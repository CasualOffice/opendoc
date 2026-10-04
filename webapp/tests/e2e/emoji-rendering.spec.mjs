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

// The asset is `Noto-COLRv1.ttf`.
//
// Matched by FILENAME, which is the one thing both legitimate sources agree on.
//
// This matcher used to be `/googlefonts\/noto-emoji@/i` — the upstream
// REPOSITORY — on the reasoning that a repository outlives a filename. `109`
// HF-176 made that a coin toss: the colour-emoji face is served from our own
// origin on a deployment that has provisioned it and from the pinned upstream
// mirror on one that has not, so a matcher naming either source alone passes or
// fails on whether the machine running it happens to hold a 4.99 MB file.
// Pinning the origin instead of the repository would have been the same mistake
// with the sides swapped. The filename is true in both states, and this spec is
// about whether the face is ASKED FOR at all, not about where from —
// `cjk-fonts.spec.mjs` is where the origin itself is held.
//
// This matcher has now been left behind by a filename twice (`/notoemoji/i`
// against the monochrome Google Fonts path, then `/NotoColorEmoji/i` against
// the CBDT build), each time recording zero requests while the editor fetched
// the font correctly all along — so if the ship build ever changes, change this
// with it.
const EMOJI_FONT = /Noto-COLRv1\.ttf(?:[?#]|$)/i;

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
  // One of the two legitimate sources, and nowhere else. Which one depends on
  // whether this deployment declared the face provisioned; both are verified
  // against the manifest's SHA-256 before the bytes reach the engine.
  expect(fontRequests[0]).toMatch(
    /\/assets\/fonts\/script\/Noto-COLRv1\.ttf|cdn\.jsdelivr\.net\/gh\/googlefonts\/noto-emoji@[0-9a-f]{40}\//,
  );

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
