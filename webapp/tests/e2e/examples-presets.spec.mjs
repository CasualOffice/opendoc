import { test, expect } from "./fixtures.mjs";

// Regression: every gallery demo (editor.html?demo=<name>) must land in a
// DISTINCT, meaningful state, not sit on the plain editor. The preset chrome is
// applied the moment the document opens (before the network font fetch), so
// these pure-DOM signals are asserted with polling rather than a fixed wait.

test("gallery demos open the editor in distinct, capability-specific states", async ({ page }) => {
  // Tables -> the Insert ribbon tab is selected (not the default Home).
  await page.goto("/editor.html?demo=tables");
  await expect
    .poll(() => page.evaluate(() => document.querySelector('.ribbon-tab[aria-selected="true"]')?.dataset.tab))
    .toBe("insert");

  // Tracked changes -> Suggesting mode (its banner shows).
  await page.goto("/editor.html?demo=changes");
  await expect(page.locator("#suggestingBanner")).toBeVisible();

  // Find -> the find panel is open.
  await page.goto("/editor.html?demo=find");
  await expect(page.locator("#findPanel")).toBeVisible();

  // Export -> the View ribbon tab is selected.
  await page.goto("/editor.html?demo=export");
  await expect
    .poll(() => page.evaluate(() => document.querySelector('.ribbon-tab[aria-selected="true"]')?.dataset.tab))
    .toBe("view");
});
