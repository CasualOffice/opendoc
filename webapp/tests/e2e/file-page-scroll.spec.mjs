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
import { expect, stableBox, test } from "./fixtures.mjs";

// Panes with more content than a short window. `Export` and `New document` are
// deliberately absent: they size to their grid and a test that asserts scrolling
// on a pane that fits would be asserting nothing.
const TALL_PANES = ["Settings", "Page setup", "Document properties", "Keyboard shortcuts"];

// The test CREATES the overflow it needs instead of depending on how tall the
// dialogs happen to be, because that dependency broke it.
//
// It ran at 1280x720 and asserted each pane overflowed there. #616 then made the
// dialogs genuinely shorter — Settings' card 672 -> 565, Document properties'
// 672 -> 460 — and at 720 both panes came to fit EXACTLY (662/662 measured), so
// the precondition failed and `main` went red on a change that had improved the
// product. That is a test pinned to a circumstance rather than to the guarantee:
// the guarantee is "a pane taller than its window can be scrolled to the bottom",
// and nothing about it says which window size makes a pane taller.
//
// 520px is a real short window — a split screen, or a laptop with the dock and a
// browser toolbar — and at it all four panes overflow with room to spare
// (measured on this fixture: 595/462, 767/462, 514/462, 726/462). The
// per-pane precondition below is KEPT so that if a future change makes even this
// window big enough for a pane, the test says so instead of quietly passing.
const SHORT_WINDOW = { width: 1280, height: 520 };

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
  test.use({ viewport: SHORT_WINDOW });

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
      expect(overflows, `${name} must overflow at ${SHORT_WINDOW.height}px for this test to mean anything`).toBe(true);

      await pane.evaluate((el) => {
        el.scrollTop = 0;
      });
      const box = await stableBox(pane);
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
