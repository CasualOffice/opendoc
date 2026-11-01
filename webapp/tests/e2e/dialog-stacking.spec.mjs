// Never a modal over a modal (`docs/184` D1a; the owner, 2026-11-01: "popup
// over a popup — who designed it").
//
// Settings opened AI provider settings and Manage add-ins as second modals
// painted over it, and Manage add-ins opened the grant prompt over itself: two
// scrims, two cards, two titles. `modal.mjs` now puts the first dialog away
// while the second is up and brings it back, untouched, when the second closes,
// so exactly one `.dialog-overlay` is painted at any time. Every `.dialog-close`
// in a registered modal closes it: the AI provider's ✕ was wired to nothing.
import { expect, gotoEditor, test } from "./fixtures.mjs";
import { DIALOGS } from "./modal-roster.mjs";

const visibleOverlays = (page) =>
  page.evaluate(
    () => [...document.querySelectorAll(".dialog-overlay")].filter((el) => el.getClientRects().length > 0).length,
  );

test("a dialog opened from a dialog takes its place, and the first comes back on close", async ({ page }) => {
  await gotoEditor(page);
  await page.locator("#settingsBtn").click();
  await expect(page.locator("#settingsPanel")).toBeVisible();
  expect(await visibleOverlays(page)).toBe(1);

  await page.locator("#aiProviderSettingsBtn").click();
  await expect(page.locator("#aiProviderDialog")).toBeVisible();
  expect(await visibleOverlays(page), "AI provider is painted over Settings").toBe(1);
  await expect(page.locator("#settingsPanel")).toBeHidden();

  // Its ✕ closes it and Settings is back where it was.
  await page.locator("#aiProviderDialog .dialog-close").click();
  await expect(page.locator("#aiProviderDialog")).toBeHidden();
  await expect(page.locator("#settingsPanel")).toBeVisible();
  expect(await visibleOverlays(page)).toBe(1);

  await page.keyboard.press("Escape");
  await expect(page.locator("#settingsPanel")).toBeHidden();
  expect(await visibleOverlays(page)).toBe(0);
});

// NO SCROLLER UNLESS THE CONTENT NEEDS ONE (the owner, 2026-11-01: "every
// dialog now shows a scroller"). At a 1280x800 laptop only the dialogs listed
// here may scroll, each with its reason.
const KNOWN_LONG = new Map([["shortcutsDialog", "the keymap is a reference meant to be scrolled"]]);

for (const dialog of DIALOGS()) {
  test(`${dialog.name}: no scrollbar at 1280x800 unless needed, and its ✕ closes it`, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await dialog.open(page);
    const root = page.locator(`#${dialog.id}`);
    await expect(root).toBeVisible();
    const fit = await page.evaluate((id) => {
      const body = document.querySelector(`#${id} .dialog-body`);
      return body ? { scroll: body.scrollHeight, client: body.clientHeight } : null;
    }, dialog.id);
    if (fit && !KNOWN_LONG.has(dialog.id)) {
      expect(fit.scroll, `#${dialog.id} scrolls at 1280x800: ${fit.scroll}px in ${fit.client}px`).toBeLessThanOrEqual(
        fit.client,
      );
    }
    expect(await visibleOverlays(page), "only one dialog is painted").toBe(1);
    const close = root.locator(".dialog-close").first();
    if ((await close.count()) > 0 && (await close.isVisible())) {
      await close.click();
      await expect(root).toBeHidden();
    }
  });
}
