// RC-16: Ctrl+Enter (Cmd+Enter on a Mac) posts a comment, as Word's "Post
// comment" does, and Shift+Enter stays a newline.
//
// The composer used to refuse every modified Enter outright, so the gesture a
// reader who learned comments in Word presses did nothing at all. Driven with
// real key presses through `attachComposerKeys`, the one key contract every
// comment box shares (`review_chrome.mjs`).
import { expect, stableBox, test } from "./fixtures.mjs";

const markers = (page) => page.locator(".review-comment-marker");

async function openComposer(page) {
  await page.goto("/editor.html?fixture=rich");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
  const sheet = page.locator(".page-wrap .page").first();
  const box = await stableBox(sheet);
  await page.mouse.click(box.x + box.width * 0.25, box.y + box.height * 0.12);
  for (let i = 0; i < 10; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.locator('[data-tab="review"]').click();
  await page.locator("#reviewCommentBtn").click();
  const composer = page.getByTestId("review-comment-composer");
  await expect(composer).toBeFocused();
  return composer;
}

for (const chord of ["Control+Enter", "Meta+Enter"]) {
  test(`${chord} posts the comment`, async ({ page, consoleErrors }) => {
    const composer = await openComposer(page);
    await page.keyboard.type("Posted with a chord");
    await page.keyboard.press(chord);
    await expect(markers(page)).toHaveCount(1);
    await expect(composer).toHaveCount(0);
    await expect(page.locator(".review-margin-comment .review-margin-body").first()).toHaveText(
      "Posted with a chord",
    );
    expect(consoleErrors).toEqual([]);
  });
}

test("Shift+Enter is a newline, not a post", async ({ page, consoleErrors }) => {
  const composer = await openComposer(page);
  await page.keyboard.type("first");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("second");
  await expect(composer).toHaveValue("first\nsecond");
  await expect(markers(page)).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});
