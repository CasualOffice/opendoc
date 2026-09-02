// HF-018 (docs/104-HOTFIX-TRACKER.md): with cookies and site data blocked — or
// in a cross-origin embed, where the same restriction applies — touching
// `window.localStorage` throws. `main.js` read a preference at module scope with
// no guard, so the throw aborted evaluation of the entire module before a single
// listener was attached: the page loaded, painted nothing, and answered no
// keyboard or ribbon input, with only a console error to say why.
//
// The browser cannot be told to block site data from Playwright, so this spec
// reproduces the exact failure mode by making every localStorage access throw
// the SecurityError a blocking browser raises, before any script runs.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

async function blockSiteData(page) {
  await page.addInitScript(() => {
    const deny = () => {
      throw new DOMException("Access is denied for this document.", "SecurityError");
    };
    // Property access itself throws when site data is blocked, which is why a
    // try/catch around only `getItem` would not have been enough.
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      get: deny,
    });
  });
}

test("the editor is fully live with site data blocked", async ({ page, consoleErrors }) => {
  await blockSiteData(page);
  await gotoEditor(page);

  // Chrome is wired: a document opened and rendered at all, which already
  // requires the module to have finished evaluating.
  await expect(page.locator(".page-wrap")).not.toHaveCount(0);
  await expect(page.locator("#save")).toBeEnabled();

  // Editing is wired: type, then undo through the ribbon control.
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("STORAGEBLOCKED");
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("STORAGEBLOCKED");
  await expect(page.locator("#findStatus")).toHaveText("1 match");
  await page.keyboard.press("Escape");
  await page.locator("#undoBtn").click();

  // The palette — the keyboard fallback for every command — is wired too.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.keyboard.press("Escape");

  // A preference that cannot be persisted still applies for the session: smart
  // quotes default to on, so the typed straight quotes are curled.
  await clickIntoFirstPage(page);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type('"Hello"');
  await expect
    .poll(() =>
      page.evaluate(() => {
        const el = [...document.querySelectorAll("#a11yDocument p")].find((node) =>
          (node.textContent || "").includes("Hello"),
        );
        return el ? el.textContent : null;
      }),
    )
    .toBe("“Hello”");

  expect(consoleErrors).toEqual([]);
});
