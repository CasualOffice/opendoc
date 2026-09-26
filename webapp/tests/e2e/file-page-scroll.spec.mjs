// A File-page pane taller than the window must scroll.
//
// It did not. Every pane was unreachable below the fold, which the owner hit
// first on Page setup — "how do i set things".
//
// The cause was one level too deep to guess at, and worth recording because the
// shape recurs. Panels are BORROWED into the pane: the same element is a modal
// over a scrim one moment and a section of a page the next. As a modal,
// `.dialog-body` is correctly `overflow-y: auto` with `overscroll-behavior:
// contain` — it scrolls itself and must not rubber-band the document behind it.
// Borrowed into the pane it becomes a scroll container with NOTHING to scroll
// (its content fits it exactly), and `contain` on a non-scrollable element
// BLOCKS scroll chaining in Chromium. Walking the chain from the wheel target:
//
//     .dialog-body     auto / contain   scrollable: false   <- swallowed here
//     .dialog-card     hidden           scrollable: false
//     #filePageDetail  auto / contain   scrollable: TRUE    <- never reached
//
// So the wheel died two levels below the only element that could move, while
// Settings had 877px of content in a 662px pane.
//
// This asserts the GUARANTEE — the reader can reach the bottom — rather than any
// particular overflow value, because the next person to restyle a dialog will
// change the values and should still be caught.
import { test, expect } from "./fixtures.mjs";

// Panes whose content is genuinely taller than a laptop viewport. `Export` and
// `New document` are deliberately absent: they size to their grid and a test
// that asserts scrolling on a pane that fits would be asserting nothing.
const TALL_PANES = ["Settings", "Page setup", "Document properties", "Keyboard shortcuts"];

async function openFilePage(page) {
  await page.goto("/editor.html?fixture=rich");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
  await page.locator(".page-wrap .page").first().click({ position: { x: 200, y: 120 } });
  await page.locator('[data-tab="file"]').click();
  await expect(page.locator("#filePageBody")).toBeVisible();
}

test.describe("File page panes scroll", () => {
  test.use({ viewport: { width: 1280, height: 720 } });

  for (const name of TALL_PANES) {
    test(`${name}: the wheel reaches content below the fold`, async ({ page, consoleErrors }) => {
      await openFilePage(page);
      const row = page.locator(`#filePageBody button:has-text("${name}")`).first();
      await expect(row).toBeVisible();
      await row.click();

      const pane = page.locator("#filePageDetail");
      await expect(pane).toBeVisible();

      // Precondition: this pane really does overflow at this size, or the test
      // below would pass on a pane with nothing to scroll.
      const overflows = await pane.evaluate((el) => el.scrollHeight > el.clientHeight + 1);
      expect(overflows, `${name} must overflow at 720px for this test to mean anything`).toBe(true);

      await pane.evaluate((el) => {
        el.scrollTop = 0;
      });
      const box = await pane.boundingBox();
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.mouse.wheel(0, 400);

      await expect
        .poll(async () => await pane.evaluate((el) => el.scrollTop), {
          message: `the wheel did not scroll the ${name} pane — content below the fold is unreachable`,
        })
        .toBeGreaterThan(0);

      expect(consoleErrors).toEqual([]);
    });
  }

  test("a borrowed panel does not become a dead scroll container", async ({ page }) => {
    // The mechanism, guarded directly as well as by behaviour: in the pane the
    // panel must not be a scroller that can swallow a wheel it cannot use.
    await openFilePage(page);
    await page.locator('#filePageBody button:has-text("Settings")').first().click();
    await expect(page.locator("#filePageDetail .panel-in-page")).toBeVisible();

    const swallowers = await page.evaluate(() => {
      const found = [];
      for (const el of document.querySelectorAll("#filePageDetail .panel-in-page, #filePageDetail .panel-in-page *")) {
        const s = getComputedStyle(el);
        const scrolls = /auto|scroll/.test(s.overflowY);
        const canScroll = el.scrollHeight > el.clientHeight + 1;
        if (scrolls && !canScroll && s.overscrollBehaviorY === "contain") {
          found.push(`${el.tagName}.${(el.className || "").toString().split(" ")[0]}`);
        }
      }
      return found;
    });
    expect(
      swallowers,
      "these elements scroll-contain without being scrollable, which blocks the wheel from reaching the pane",
    ).toEqual([]);
  });
});
