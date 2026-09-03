// The Styles gallery draws each card's label in the style it represents, colour
// included — Word's behaviour. Word can do that unconditionally because its
// gallery sits on a permanently light background. Ours follows the theme, and a
// document's colours are absolute: Word's own built-in Heading 1 is #2F5496,
// which is 2.1:1 on the dark surface. The label was painted, correctly, in a
// colour nobody could see.
//
// Every shipped sample leaves its style colours automatic — they resolve to an
// empty string and inherit the theme's ink — so the whole suite passed while any
// real .docx reproduced this on the first load. `?fixture=styled` exists to close
// that hole: its styles carry explicit colours, exactly as Word writes them.
import { test, expect } from "./fixtures.mjs";

async function gotoStyled(page) {
  await page.goto("/editor.html?fixture=styled");
  await page.waitForFunction(
    () => document.querySelectorAll("#stylesGallery .style-card").length > 1,
    null,
    { timeout: 45_000 },
  );
}

/** The contrast every gallery label actually renders at, measured through the
 *  same compositing the browser does. */
async function labelContrasts(page) {
  return page.evaluate(() => {
    const ctx = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
    const parse = (value) => {
      ctx.fillStyle = "#ff00ff";
      ctx.fillStyle = value;
      ctx.clearRect(0, 0, 1, 1);
      ctx.fillRect(0, 0, 1, 1);
      const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
      return { r, g, b, a: a / 255 };
    };
    const lum = (c) => {
      const f = (v) => (v / 255 <= 0.03928 ? v / 255 / 12.92 : ((v / 255 + 0.055) / 1.055) ** 2.4);
      return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
    };
    const over = (fg, bg) => ({
      r: fg.r * fg.a + bg.r * (1 - fg.a),
      g: fg.g * fg.a + bg.g * (1 - fg.a),
      b: fg.b * fg.a + bg.b * (1 - fg.a),
      a: 1,
    });
    const backdrop = (el) => {
      for (let n = el; n && n.nodeType === 1; n = n.parentElement) {
        const c = parse(getComputedStyle(n).backgroundColor);
        if (c.a === 1) return c;
      }
      return parse(getComputedStyle(document.body).backgroundColor);
    };
    return [...document.querySelectorAll("#stylesGallery .style-card-name")].map((label) => {
      const bg = backdrop(label);
      const fg = over(parse(getComputedStyle(label).color), bg);
      const [hi, lo] = [lum(fg), lum(bg)].sort((a, b) => b - a);
      return {
        name: label.textContent.trim(),
        authored: label.dataset.previewColor || "",
        ratio: (hi + 0.05) / (lo + 0.05),
      };
    });
  });
}

test("a document's own style colours are previewed where they are readable", async ({
  page,
  consoleErrors,
}) => {
  await gotoStyled(page);

  const cards = await labelContrasts(page);
  const coloured = cards.filter((card) => card.authored);
  // If the fixture ever stops carrying explicit colours this test proves nothing,
  // so say so loudly rather than passing vacuously.
  expect(coloured.length, "the styled fixture no longer has coloured styles").toBeGreaterThan(0);

  // On the light theme these colours are readable, so they must be used AS
  // AUTHORED. A "fix" that simply stopped previewing colours would satisfy the
  // dark-theme test below while quietly deleting the feature.
  for (const card of coloured) {
    const used = await page
      .locator(`#stylesGallery .style-card-name`, { hasText: card.name })
      .first()
      .evaluate((el) => el.style.color);
    expect(used, `"${card.name}" stopped previewing its authored colour`).not.toBe("");
  }

  expect(consoleErrors).toEqual([]);
});

test("no style card is unreadable after switching to the dark theme", async ({
  page,
  consoleErrors,
}) => {
  await gotoStyled(page);

  // Through the real control, not by setting the attribute: the palette swap and
  // the re-decision have to be wired to each other, and poking the DOM directly
  // would skip precisely the wiring under test.
  await page.locator("#settingsBtn").click();
  await page.locator('#themeSeg button[data-theme="dark"]').click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  const unreadable = (await labelContrasts(page)).filter((card) => card.ratio < 3);
  expect(
    unreadable.map((card) => `${card.name} at ${card.ratio.toFixed(2)}:1 (${card.authored})`),
    "style cards are painted in a colour that cannot be read on the dark theme",
  ).toEqual([]);

  expect(consoleErrors).toEqual([]);
});
