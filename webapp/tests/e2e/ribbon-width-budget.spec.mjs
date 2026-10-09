// What the ribbon width budget ACTUALLY is, measured rather than remembered.
//
// `SKILL.md` §11 said "there is ~55px of slack on the Home band; widening one
// control exiles a whole group into the `⋯` overflow", and
// `ribbon-home.spec.mjs` said the band "had 10px of slack at 1280px". Both
// figures were wrong, and — which matters more — so was the MECHANISM. Measured
// in Chromium at 1280x900:
//
//   * The Home band fits down to a 1017px viewport with nothing exiled, so it has
//     roughly 263px of viewport headroom at 1280, not 55 and not 10. The other six
//     bands fit down to 702-743px.
//   * Growing a Home group by 288px changes nothing about the `⋯` button. What
//     breaks first is HORIZONTAL SCROLL: `stoppedBecause: {exiled: false,
//     hscroll: true}`. The overflow button responds to viewport width, not to
//     content width, so "widening a control exiles a group" does not describe this
//     ribbon at all.
//
// The consequence of getting this wrong was not academic: the 55px figure was
// quoted to reject ribbon additions, so a five-fold underestimate was making
// capabilities unreachable on purpose. A wrong number in a rule is worse than no
// rule, because it gets obeyed.
//
// So this file DERIVES the budget instead of restating it. `SKILL.md` now carries
// the recipe and points here for the number, which is the only arrangement that
// cannot go stale: the next person to widen a control gets an answer from the
// browser rather than from a sentence somebody wrote in September.
import { test, expect, gotoEditor } from "./fixtures.mjs";

const AT = { width: 1280, height: 900 };

/** True when the band is fully laid out: nothing exiled to `⋯`, and no scroller
 *  on the band, its parent, or the document. BOTH halves matter — measuring only
 *  the overflow button is what produced the wrong mechanism. */
async function bandFits(page) {
  return page.evaluate(() => {
    const band = document.querySelector("#panelHome");
    if (!band) return { fits: false, why: "no #panelHome" };
    const btn = document.querySelector("#ribbonOverflowBtn");
    const exiled = !!(
      btn &&
      !(btn.hidden || getComputedStyle(btn).display === "none" || btn.offsetParent === null)
    );
    const parent = band.parentElement;
    const hscroll =
      band.scrollWidth > band.clientWidth + 1 ||
      (!!parent && parent.scrollWidth > parent.clientWidth + 1) ||
      document.documentElement.scrollWidth > document.documentElement.clientWidth + 1;
    return { fits: !exiled && !hscroll, exiled, hscroll };
  });
}

test.describe("the ribbon width budget", () => {
  test("the Home band fits 1280px, and the budget is a measured number", async ({ page }) => {
    await page.setViewportSize(AT);
    await gotoEditor(page);
    await page.locator('[data-tab="home"]').click();

    const before = await bandFits(page);
    expect(
      before.fits,
      `the Home band does not fit 1280px as shipped (${JSON.stringify(before)}) — that is the ` +
        "rule this file exists to protect, and it is already broken",
    ).toBe(true);

    // How much a group can grow before the band stops fitting. This is the
    // quantity the rule is about, and nothing was measuring it.
    const headroom = await page.evaluate(async () => {
      const band = document.querySelector("#panelHome");
      const groups = [...band.querySelectorAll(".rgroup")];
      const group = groups[groups.length - 1];
      const fits = () => {
        const btn = document.querySelector("#ribbonOverflowBtn");
        const exiled = !!(
          btn &&
          !(btn.hidden || getComputedStyle(btn).display === "none" || btn.offsetParent === null)
        );
        const parent = band.parentElement;
        return (
          !exiled &&
          !(
            band.scrollWidth > band.clientWidth + 1 ||
            (!!parent && parent.scrollWidth > parent.clientWidth + 1) ||
            document.documentElement.scrollWidth > document.documentElement.clientWidth + 1
          )
        );
      };
      const pad = document.createElement("span");
      pad.style.cssText = "display:inline-block;height:1px;";
      group.appendChild(pad);
      let grew = 0;
      for (let w = 0; w <= 1000; w += 4) {
        pad.style.width = `${w}px`;
        await new Promise((r) => requestAnimationFrame(r));
        if (!fits()) break;
        grew = w;
      }
      pad.remove();
      return grew;
    });

    // A FLOOR, not an equality. The point of a budget is that it can be spent:
    // adding a control is allowed and should lower this number. What is not
    // allowed is spending it down to nothing without anybody noticing, which is
    // how a band ends up one control away from a horizontal scrollbar.
    //
    // 288px measured today. The floor is 120px, which leaves room for roughly a
    // full extra group and still fails loudly before the band is in trouble.
    // Deliberately NOT pinned at 288: a guard that reds on any change that spends
    // budget is a guard people delete, and this repository has several.
    expect(
      headroom,
      `the Home band has only ${headroom}px of growth headroom at 1280px, below the 120px ` +
        "floor. Either reclaim width (a narrower control, a group moved to another band) or " +
        "argue the floor down — but do not widen anything further first",
    ).toBeGreaterThanOrEqual(120);

    // Published so that spending the budget is visible in a diff rather than
    // discovered later. Not asserted as an equality, for the reason above.
    console.log(`RIBBON_HOME_HEADROOM_AT_1280 ${headroom}px`);
  });

  test("every other band fits 1280px too, and its budget is published", async ({ page }) => {
    // Home was the only band measured, so the Review band could be widened —
    // as `109` UX-040 does, labelling Track changes, Accept, Reject, Spelling and
    // Grammar — with nothing to say what that spent. Each band is measured the
    // same way Home is, against the same floor, and published beside it.
    await page.setViewportSize(AT);
    await gotoEditor(page);
    const measured = {};
    for (const tab of ["insert", "layout", "references", "review", "view"]) {
      await page.locator(`[data-tab="${tab}"]`).click();
      await expect(page.locator(`.ribbon-panel[data-panel="${tab}"]`)).toBeVisible();
      measured[tab] = await page.evaluate(async (name) => {
        const band = document.querySelector(`.ribbon-panel[data-panel="${name}"]`);
        const groups = [...band.querySelectorAll(".rgroup")];
        const group = groups[groups.length - 1];
        const fits = () => {
          const btn = document.querySelector("#ribbonOverflowBtn");
          const exiled = !!(
            btn &&
            !(btn.hidden || getComputedStyle(btn).display === "none" || btn.offsetParent === null)
          );
          const parent = band.parentElement;
          return (
            !exiled &&
            !(
              band.scrollWidth > band.clientWidth + 1 ||
              (!!parent && parent.scrollWidth > parent.clientWidth + 1) ||
              document.documentElement.scrollWidth > document.documentElement.clientWidth + 1
            )
          );
        };
        if (!fits()) return -1;
        const pad = document.createElement("span");
        pad.style.cssText = "display:inline-block;height:1px;";
        group.appendChild(pad);
        let grew = 0;
        for (let w = 0; w <= 1200; w += 4) {
          pad.style.width = `${w}px`;
          await new Promise((r) => requestAnimationFrame(r));
          if (!fits()) break;
          grew = w;
        }
        pad.remove();
        return grew;
      }, tab);
    }
    for (const [tab, headroom] of Object.entries(measured)) {
      expect(
        headroom,
        `the ${tab} band has ${headroom}px of growth headroom at 1280px (-1: it does not fit at all), ` +
          "below the 120px floor Home is held to",
      ).toBeGreaterThanOrEqual(120);
      console.log(`RIBBON_${tab.toUpperCase()}_HEADROOM_AT_1280 ${headroom}px`);
    }
  });

  test("it is horizontal scroll that breaks first, not the overflow menu", async ({ page }) => {
    // The corrected mechanism, asserted so the wrong one cannot come back into a
    // comment. If a future ribbon really does exile a group on growth, this test
    // fails and the rule gets rewritten from a measurement again.
    await page.setViewportSize(AT);
    await gotoEditor(page);
    await page.locator('[data-tab="home"]').click();

    const outcome = await page.evaluate(async () => {
      const band = document.querySelector("#panelHome");
      const groups = [...band.querySelectorAll(".rgroup")];
      const pad = document.createElement("span");
      pad.style.cssText = "display:inline-block;height:1px;width:400px;";
      groups[groups.length - 1].appendChild(pad);
      await new Promise((r) => requestAnimationFrame(r));
      const btn = document.querySelector("#ribbonOverflowBtn");
      const exiled = !!(
        btn &&
        !(btn.hidden || getComputedStyle(btn).display === "none" || btn.offsetParent === null)
      );
      const hscroll = band.scrollWidth > band.clientWidth + 1;
      pad.remove();
      return { exiled, hscroll };
    });

    expect(
      outcome.hscroll,
      "growing a group by 400px did not produce horizontal overflow, so the stated failure " +
        "mode is wrong again and the budget needs re-deriving",
    ).toBe(true);
    expect(
      outcome.exiled,
      "growing a group exiled a group to the `⋯` menu. That is NOT what this ribbon did when " +
        "measured — the overflow button tracked viewport width, not content width. If this is " +
        "now true, the mechanism changed and `SKILL.md` §11 must be updated from a measurement",
    ).toBe(false);
  });
});
