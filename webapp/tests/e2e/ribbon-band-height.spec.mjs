// Changing ribbon tab must not move the document.
//
// The band had no height of its own: `.ribbon-panel` is a flex row, so each tab
// stood exactly as tall as whatever controls it happened to contain. Measured at
// 1280px that was Home 79px (its groups carry two control rows), Insert / Layout
// / References 58px, and Review / View 48px — so every tab change jolted `#pages`
// by up to 31px under the reader's cursor, and the chrome visibly changed size
// while they were looking at it. Word and ONLYOFFICE both pin the band and align
// the groups inside it.
//
// The assertion is the GUARANTEE, not the mechanism: what a user experiences is
// "the page does not jump", so that is what is measured — the top of `#pages`,
// across every tab. A band-height check alone would keep passing if some other
// piece of chrome started varying instead.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

/** Every non-contextual band. `file` is excluded because it is a PAGE rather
 *  than a band (`one-axis-navigation.spec.mjs` owns that distinction), and
 *  `table` because it is contextual and disabled without a table selected. */
const TABS = ["home", "insert", "layout", "references", "review", "view"];

async function measureTabs(page) {
  const seen = [];
  for (const tab of TABS) {
    await page.locator(`[data-tab="${tab}"]`).click();
    await expect(page.locator(`.ribbon-panel[data-panel="${tab}"]`)).toBeVisible();
    seen.push(
      await page.evaluate((name) => {
        const round = (el) => Math.round(el.getBoundingClientRect().height);
        const panel = document.querySelector(".ribbon-panel:not([hidden])");
        return {
          tab: name,
          band: round(panel),
          ribbon: round(document.querySelector(".ribbon")),
          documentTop: Math.round(document.querySelector("#pages").getBoundingClientRect().top),
        };
      }, tab),
    );
  }
  return seen;
}

test("the document does not move when the ribbon tab changes", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const seen = await measureTabs(page);

  const tops = [...new Set(seen.map((s) => s.documentTop))];
  expect(
    tops,
    `#pages sits at a different height per tab, so switching jolts the document:\n  ${seen
      .map((s) => `${s.tab}: documentTop ${s.documentTop} (band ${s.band})`)
      .join("\n  ")}`,
  ).toHaveLength(1);
});

test("every ribbon band is the same height", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const seen = await measureTabs(page);

  const heights = [...new Set(seen.map((s) => s.band))];
  expect(
    heights,
    `the bands disagree about how tall a band is:\n  ${seen
      .map((s) => `${s.tab}: ${s.band}px`)
      .join("\n  ")}`,
  ).toHaveLength(1);
});

test("no control is painted outside the band that holds it", async ({ page }) => {
  // A pinned band height is only an improvement if the tallest band still FITS.
  //
  // This is the test that catches the regression the other two cannot see: turn
  // `min-height` into a plain `height` that is too small for Home and the bands
  // are still all the same height AND the document still does not move, so both
  // tests above stay green while Home's controls are painted straight through
  // the bottom of the card. Verified by doing exactly that (`height: 48px`) —
  // only this one goes red.
  //
  // Note it does NOT fire for a merely oversized control while the rule is
  // `min-height`, because the panel grows to hold it. That is the correct
  // behaviour, not a gap: growing is what keeps Home from clipping, and the
  // growth itself is what the "same height" test reports.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  for (const tab of TABS) {
    await page.locator(`[data-tab="${tab}"]`).click();
    await expect(page.locator(`.ribbon-panel[data-panel="${tab}"]`)).toBeVisible();
    const spilling = await page.evaluate(() => {
      const panel = document.querySelector(".ribbon-panel:not([hidden])");
      const box = panel.getBoundingClientRect();
      return [...panel.querySelectorAll(".rgroup, button, select, input")]
        .filter((el) => {
          const r = el.getBoundingClientRect();
          if (r.height === 0) return false;
          return r.top < box.top - 1 || r.bottom > box.bottom + 1;
        })
        .map((el) => `${el.id || el.className}: ${Math.round(el.getBoundingClientRect().height)}px`);
    });
    expect(spilling, `${tab} paints controls outside its band`).toEqual([]);
  }
});
