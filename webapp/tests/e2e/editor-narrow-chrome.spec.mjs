// Two chrome defects a narrow window produces, and the rules that hold them
// shut: `109` HF-232 (the left rail) and `109` HF-233 (the status toast over a
// drawer's footer action).
//
// Both were filed from a screenshot at 390px and neither had a guard, so both
// were free to come back. What they have in common is that the editor's chrome
// is a LADDER of widths — the footer sheds counts, the buttons shed labels, the
// side panels become drawers — and each of these two is a rung somebody built
// without asking what else is on the bottom edge at that width.
//
// EVERY NUMBER HERE IS MEASURED IN THE BROWSER, not carried over from the
// report. HF-232 was filed as "the captions clip: Outlin…, Commen…, Version…",
// and that symptom no longer exists: `.rail-btn` is `width: auto` now, so the
// caption does not ellipsise — the TILE grows instead, and it grows to the
// longest caption in the active locale. Measured at a 320px viewport, the rail
// is 66px in English, 86px in Russian and 111px in Brazilian Portuguese, which
// is 35% of a phone screen spent on four destinations whose glyphs fit in 36px.
// So the defect is real, the remedy the row names is the right one (Word and
// Docs both drop a rail caption on a narrow window), and the assertion below is
// about the harm rather than about the ellipsis that used to reveal it.
import { expect, gotoEditor, runFilePageCommand, test } from "./fixtures.mjs";

/** The four rail tiles, as the markup declares them. */
const RAIL = ["#railOutline", "#railPages", "#railReview", "#railVersions"];

/** The rail's own width, and the caption box inside each tile. */
function railMetrics(page) {
  return page.evaluate(() => {
    const rail = document.querySelector(".rail");
    return {
      railWidth: Math.round(rail.getBoundingClientRect().width),
      // Whether the rail is on screen at all. The phone rung withholds it
      // (docs/148 §5.3a) and a withheld tile measures 0 — which reads as "a
      // control that got smaller" unless the guard can tell the two apart.
      railPainted: getComputedStyle(rail).display !== "none",
      viewport: window.innerWidth,
      // What the DOCUMENT is left with. The rail's own width stopped being the
      // way to measure that when the phone tier turned the rail on its side
      // (docs/148 §5.2): a horizontal strip is as wide as the window and costs
      // the document nothing, so `railWidth` now answers a different question
      // from the one this guard asks.
      documentWidth: Math.round(document.getElementById("viewport").clientWidth),
      // Language-invariant iff the tiles are glyphs. Captions are what made a
      // tile wider in Brazilian Portuguese than in English, and the SUM of the
      // tiles is the same number whichever axis they are laid out on.
      tileWidthTotal: [...document.querySelectorAll(".rail-btn")].reduce(
        (sum, tile) => sum + Math.round(tile.getBoundingClientRect().width),
        0,
      ),
      tiles: [...document.querySelectorAll(".rail-btn")].map((tile) => {
        const caption = tile.querySelector("span:not(.ms)");
        const box = tile.getBoundingClientRect();
        return {
          id: tile.id,
          name: tile.getAttribute("aria-label") ?? "",
          localised: tile.hasAttribute("data-i18n-label"),
          captionShown: caption ? getComputedStyle(caption).display !== "none" : false,
          width: Math.round(box.width),
          height: Math.round(box.height),
        };
      }),
    };
  });
}

test("a narrow window spends nothing on rail captions, and loses no destination doing it", async ({
  page,
}) => {
  // REWRITTEN when the phone rung stopped painting the rail at all (docs/148
  // §5.3a). The previous version measured the rail's tiles at 390px, and it is
  // the shape `SKILL.md` warns about: a guard pinned to the CIRCUMSTANCE it was
  // written in — "there is a rail here, and it is made of glyphs" — reddens
  // when a change removes the circumstance while strengthening the guarantee.
  // At 390 the document now gets the whole window instead of five sixths of it,
  // and the old assertions failed because a withheld tile measures 0.
  //
  // The guarantee has three parts and none of them names a width:
  //   1. the rail carries its words where there is room for them;
  //   2. where there is not, it costs the same in every language, because it
  //      has stopped being made of words — a caption-sized tile is 66px in
  //      English, 86px in Russian and 111px in Brazilian Portuguese;
  //   3. dropping the caption silences nothing: every destination keeps a
  //      translated accessible name, and on a phone, where the tile is not
  //      painted at all, a menu row carrying the same name.
  //
  // Two languages whose captions differ by 45px of tile: English and Brazilian
  // Portuguese ("Estrutura de tópicos"). English alone could not tell a glyph
  // rail from a caption rail.
  const measured = {};
  for (const tag of ["en", "pt-BR"]) {
    // The width comes FIRST, and on the second pass that matters: the loop's
    // last act is to shrink to 390, where the rail is not painted, so a
    // `waitForSelector` (which waits for visibility) would hang waiting for a
    // tile this rung deliberately withholds.
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto(`/editor.html?lang=${tag}`);
    await page.waitForSelector("#railOutline");

    // (1) Wide: the rail carries its words. This is what proves the shed is a
    // decision about space rather than a caption somebody deleted.
    const wide = await railMetrics(page);
    expect(wide.tiles.every((tile) => tile.captionShown)).toBe(true);

    // (2) The rung where the rail is painted and its space is contested: the
    // review column in the margin, which is the surviving caption-shed rule
    // (`:root:has(#viewport.has-review-sidebar:not(.review-sheet))`). 860 is a
    // tablet in landscape — wide enough that the column stays a margin rather
    // than becoming a sheet, narrow enough that 111px of Portuguese comes
    // straight off the page.
    await page.setViewportSize({ width: 860, height: 900 });
    await page.locator("#railReview").click();
    await expect(page.locator("#reviewSidebar")).toBeVisible();
    const contested = await railMetrics(page);
    measured[tag] = contested;

    for (const tile of contested.tiles) {
      expect(tile.captionShown, `${tag} ${tile.id} caption`).toBe(false);
      // The glyph carries the meaning on screen; the NAME has to survive, or
      // dropping the caption would silence the control for a screen reader.
      expect(tile.name.trim(), `${tag} ${tile.id} accessible name`).not.toBe("");
      expect(tile.localised, `${tag} ${tile.id} name goes through the seam`).toBe(true);
      // Still a touch target: 24px is the floor every control in this shell is
      // held to (WCAG 2.5.8 Target Size (Minimum), Level AA).
      expect(tile.width, `${tag} ${tile.id} width`).toBeGreaterThanOrEqual(24);
      expect(tile.height, `${tag} ${tile.id} height`).toBeGreaterThanOrEqual(24);
    }

    // (3) And the phone, where the rail is not painted at all. The guarantee is
    // stated about the DOCUMENT, so it holds under either answer and still
    // catches a rail made of words: the previous version measured 326 of 390
    // with a column and 390 with a strip against a floor of 325, and a withheld
    // rail can only improve on that.
    await page.setViewportSize({ width: 390, height: 844 });
    const phone = await railMetrics(page);
    expect(phone.railPainted, `${tag}: the phone rung withholds the rail`).toBe(false);
    expect(
      phone.documentWidth,
      `${tag}: the rail must leave the document at least five sixths of the window`,
    ).toBeGreaterThanOrEqual(Math.round((phone.viewport * 5) / 6));
    // Nothing became unreachable by becoming unpainted: each destination has a
    // menu row, carrying a name, in the language under test.
    for (const [menu, command] of [
      ["view", "view.outline"],
      ["view", "view.pages"],
      ["review", "review.toggle"],
      ["file", "file.versionHistory"],
    ]) {
      await page.locator(`.app-menu-button[data-menu="${menu}"]`).click();
      const row = page.locator(`#appMenuPopover .app-menu-item[data-command="${command}"]`);
      await expect(row, `${tag}: ${command} has a menu home on a phone`).toBeVisible();
      expect((await row.innerText()).trim(), `${tag}: ${command} is named`).not.toBe("");
      await page.keyboard.press("Escape");
    }
  }

  // The point of the fix, stated as the guarantee rather than as a number: a
  // contested rail is the same size whatever language it is in, because it has
  // stopped being made of words. Measured as the SUM OF THE TILES rather than
  // as the rail's box — a full-width strip is 390px in every language whether
  // or not it carries captions, so the rail's own width cannot tell them apart.
  expect(measured["pt-BR"].tileWidthTotal).toBe(measured.en.tileWidthTotal);
  // And it really did cost something at a desktop width — otherwise this whole
  // test would be asserting that a caption nobody has is still missing.
  expect(RAIL).toHaveLength(4);
});

test("a status toast never covers the footer action of an open drawer", async ({ page }) => {
  // A phone window: the side panel is a drawer pinned to the right edge, the
  // footer's informational half is gone, and the toast is therefore the ONLY
  // visible channel — which is exactly why it must not land on the one control
  // the drawer pins to its own bottom edge (`109` HF-233).
  await gotoEditor(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await runFilePageCommand(page, "file.versionHistory");
  await expect(page.locator("#versionPanel")).toBeVisible();
  await expect(page.locator("#versionPanelBody .version-item")).toHaveCount(1);

  // A REAL message on the real channel, produced by a real action on this very
  // panel: naming a version publishes "Named …", and at 390px the status line is
  // shed so every publish becomes a toast. Nothing here dispatches a synthetic
  // event at the channel — a toast the product does not produce proves nothing.
  await page.keyboard.press("End");
  await page.keyboard.press("F2");
  await expect(page.locator("#versionNameInput")).toBeFocused();
  await page.locator("#versionNameInput").fill("Before the legal review");
  await page.locator("#versionNameConfirm").click();

  const toast = page.locator("#statusToast");
  await expect(toast).toBeVisible();
  const overlap = await page.evaluate(() => {
    const rect = (selector) => document.querySelector(selector).getBoundingClientRect();
    const intersects = (a, b) =>
      a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
    const card = rect("#statusToast");
    return {
      onTheAction: intersects(card, rect("#versionClearBtn")),
      onTheDrawer: intersects(card, rect("#versionPanel")),
      toastHeight: Math.round(card.height),
      band: Number.parseInt(
        getComputedStyle(document.documentElement).getPropertyValue("--h-toast-band"),
        10,
      ),
    };
  });
  // The guarantee HF-233 names: the drawer's pinned action stays readable.
  expect(overlap.onTheAction).toBe(false);
  // And the stronger form the reserved band buys — the two fixed surfaces do not
  // share a pixel — with its precondition made explicit rather than assumed: it
  // holds for a toast up to the band. A taller one grows over the drawer's LIST,
  // which is scrollable content and not a control.
  expect(overlap.toastHeight).toBeLessThanOrEqual(overlap.band);
  expect(overlap.onTheDrawer).toBe(false);
});
