// The left rail is a column of glyphs, as Google Docs' and Word's are (`109`
// HF-331, owner: "remove names below icons on left side bar.. and make it slim").
//
// The rail used to carry a caption under every icon, and a caption-sized tile
// is as wide as the longest word in the active locale: 64px in English at
// 1440, wider in Russian or Brazilian Portuguese, all of it taken from the
// document. The guarantee this file holds is what the owner asked for, stated
// without naming the mechanism that delivers it:
//
//   1. no rail destination paints any words — the glyph is the only ink;
//   2. the rail is slim on a desktop: at most 44px, in every locale, with or
//      without a panel open beside it;
//   3. dropping the words silences nothing: every destination keeps a
//      translated accessible name AND a translated tooltip, both non-empty;
//   4. a coarse pointer still gets a 44px target, because a slim rail is a
//      fine-pointer economy and not a touch regression.
import { expect, gotoEditor, test } from "./fixtures.mjs";

/** What every rail destination shows and says, measured in the page. */
function railFacts(page) {
  return page.evaluate(() => {
    const rail = document.querySelector(".rail");
    const tiles = [...rail.querySelectorAll("button")];
    return {
      railWidth: rail.getBoundingClientRect().width,
      tiles: tiles.map((tile) => {
        // Every text node that paints: anything outside the aria-hidden glyph
        // that has a box and is not `display: none`. A caption hidden with
        // `visibility` or clipped to nothing still counts as gone; a caption
        // that renders at any size counts as a label.
        const walker = document.createTreeWalker(tile, NodeFilter.SHOW_TEXT);
        const painted = [];
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
          const text = node.textContent.trim();
          if (!text) continue;
          const host = node.parentElement;
          if (host.closest("[aria-hidden='true']")) continue;
          const style = getComputedStyle(host);
          const range = document.createRange();
          range.selectNodeContents(node);
          const box = range.getBoundingClientRect();
          if (style.visibility === "hidden" || box.width === 0 || box.height === 0) continue;
          painted.push(text);
        }
        const box = tile.getBoundingClientRect();
        return {
          id: tile.id,
          painted,
          name: tile.getAttribute("aria-label") ?? "",
          tooltip: tile.getAttribute("title") ?? "",
          localisedName: tile.hasAttribute("data-i18n-label"),
          localisedTooltip: tile.hasAttribute("data-i18n-title"),
          width: box.width,
          height: box.height,
        };
      }),
    };
  });
}

function expectSlimAndNamed(facts, where) {
  expect(facts.tiles.length, `${where}: the rail has its destinations`).toBeGreaterThanOrEqual(5);
  expect(facts.railWidth, `${where}: rail width`).toBeLessThanOrEqual(44);
  for (const tile of facts.tiles) {
    expect(tile.painted, `${where}: ${tile.id} paints no label text`).toEqual([]);
    expect(tile.name.trim(), `${where}: ${tile.id} accessible name`).not.toBe("");
    expect(tile.tooltip.trim(), `${where}: ${tile.id} tooltip`).not.toBe("");
    expect(tile.localisedName, `${where}: ${tile.id} name goes through the seam`).toBe(true);
    expect(tile.localisedTooltip, `${where}: ${tile.id} tooltip goes through the seam`).toBe(true);
    // WCAG 2.5.8 (AA) floor for a fine pointer.
    expect(tile.width, `${where}: ${tile.id} width`).toBeGreaterThanOrEqual(24);
    expect(tile.height, `${where}: ${tile.id} height`).toBeGreaterThanOrEqual(24);
  }
}

test("the rail is icon-only and slim, and every destination is still named", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await page.waitForSelector("#railOutline:not([disabled])");
  expectSlimAndNamed(await railFacts(page), "en 1440");

  // A panel open beside it: the selected destination shows without a label
  // (pressed state on an accent fill), and the rail does not widen.
  await page.locator("#railOutline").click();
  await expect(page.locator("#outlinePanel")).toBeVisible();
  await expect(page.locator("#railOutline")).toHaveAttribute("aria-pressed", "true");
  const pressed = await page.evaluate(() => {
    const on = getComputedStyle(document.getElementById("railOutline")).backgroundColor;
    const off = getComputedStyle(document.getElementById("railPages")).backgroundColor;
    return { on, off };
  });
  expect(pressed.on, "the selected destination is visibly distinct").not.toBe(pressed.off);
  expectSlimAndNamed(await railFacts(page), "en 1440, Outline open");

  // The longest captions this catalogue has: a rail made of words would be
  // widest here, so the same bound in Brazilian Portuguese proves it is glyphs.
  await page.goto("/editor.html?fixture=rich&lang=pt-BR");
  await page.waitForSelector("#railOutline:not([disabled])");
  const facts = await railFacts(page);
  expectSlimAndNamed(facts, "pt-BR 1440");
  // And the words really are translated, not left as English keys.
  expect(facts.tiles.find((tile) => tile.id === "railOutline").name).not.toBe("Outline");
});

test.describe("with a coarse pointer", () => {
  test.use({ hasTouch: true, viewport: { width: 1024, height: 768 } });

  test("each rail destination is a 44px target", async ({ page }) => {
    await gotoEditor(page);
    expect(await page.evaluate(() => matchMedia("(pointer: coarse)").matches)).toBe(true);
    await page.waitForSelector("#railOutline:not([disabled])");
    const facts = await railFacts(page);
    for (const tile of facts.tiles) {
      expect(tile.painted, `${tile.id} paints no label text`).toEqual([]);
      expect(tile.width, `${tile.id} width`).toBeGreaterThanOrEqual(44);
      expect(tile.height, `${tile.id} height`).toBeGreaterThanOrEqual(44);
    }
  });
});
