// Emoji in document content must resolve to a real face, not a notdef box.
//
// Emoji are ordinary content: a .docx or .odt authored anywhere else can carry
// them in a heading or a table cell, and the editor has to draw what it opened.
// The fallback registry mapped scalars only up to U+27BF, so every pictographic
// emoji resolved to NO font and rasterized as tofu — an import-fidelity bug that
// had nothing to do with how the character got into the document.
//
// The engine now bundles a monochrome emoji base, so an emoji is never tofu;
// this spec is about the COLOUR upgrade, which is the coverage-driven `emoji`
// bucket fetching the official Noto Color Emoji COLRv1 build and registering it
// through the host seam. It asserts the provisioning actually happens against
// real content, because a unit test over `fontKeyForCodePoint` alone would still
// pass if the fetch/register path never asked for the bucket.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

// The asset is `googlefonts/noto-emoji@<sha>/fonts/Noto-COLRv1.ttf`.
//
// Match the REPOSITORY, not the filename. This matcher has now been left behind
// by a filename twice: it was /notoemoji/i against the monochrome Google Fonts
// path, then /NotoColorEmoji/i against the CBDT build, and each time the spec
// recorded zero requests while the editor fetched the font correctly all along.
// `googlefonts/noto-emoji` is where the colour emoji face comes from and where
// nothing else does — the named faces are google/fonts and the script fallbacks
// are notofonts — so it identifies the thing under test without pinning which
// build of it we ship.
const EMOJI_FONT = /googlefonts\/noto-emoji@/i;

test("a document containing emoji provisions the emoji face", async ({ page, consoleErrors }) => {
  const fontRequests = [];
  page.on("request", (request) => {
    if (EMOJI_FONT.test(request.url())) fontRequests.push(request.url());
  });

  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // The demo fixture has no emoji, so nothing should have been fetched yet —
  // the bucket is coverage-driven and must not cost every document 4.99 MB.
  expect(fontRequests).toEqual([]);

  // Put emoji into the document through the picker, which routes through the
  // ordinary gated text path — the same content an import produces.
  await page.locator("#tabInsert").click();
  await page.locator('#panelInsert [data-command="insert.emoji"]').click();
  await expect(page.locator("#emojiDialog")).toBeVisible();
  await expect(page.locator("#emojiDialog")).toHaveClass(/glyph-panel/);
  await expect(page.locator("#emojiDialog")).not.toHaveAttribute("aria-modal");
  await expect(page.locator("canvas.page").first()).toBeVisible();
  await page.locator('#emojiGrid .glyph-cell[data-glyph="\u{1F600}"]').click();
  await page.keyboard.press("Escape");
  await expect(page.locator("#a11yDocument")).toContainText("\u{1F600}");

  await expect.poll(() => fontRequests.length, { timeout: 15_000 }).toBeGreaterThan(0);
  expect(fontRequests[0]).toMatch(/@[0-9a-f]{40}\//); // commit-pinned, immutable

  expect(consoleErrors).toEqual([]);
});

test("the emoji face is requested once, not per glyph", async ({ page, consoleErrors }) => {
  const fontRequests = [];
  page.on("request", (request) => {
    if (EMOJI_FONT.test(request.url())) fontRequests.push(request.url());
  });

  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator("#tabInsert").click();
  await page.locator('#panelInsert [data-command="insert.emoji"]').click();
  await expect(page.locator("#emojiDialog")).toBeVisible();
  await page.locator('#emojiGrid .glyph-cell[data-glyph="\u{1F600}"]').click();
  await expect.poll(() => fontRequests.length, { timeout: 15_000 }).toBeGreaterThan(0);

  // More emoji from the same blocks must reuse the provisioned face rather than
  // refetch several megabytes per character.
  const afterFirst = fontRequests.length;
  // More emoji, reached through the picker's search so they are found whatever
  // category they live in.
  for (const name of ["fire", "rocket"]) {
    await page.locator("#emojiSearch").fill(name);
    await page.locator("#emojiGrid .glyph-cell").first().click();
  }
  await page.waitForTimeout(1500);
  expect(fontRequests.length).toBe(afterFirst);

  expect(consoleErrors).toEqual([]);
});
