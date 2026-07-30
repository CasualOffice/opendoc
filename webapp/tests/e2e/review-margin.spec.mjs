import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

async function pastePlainText(page, text) {
  await page.evaluate((value) => {
    const data = new DataTransfer();
    data.setData("text/plain", value);
    document.dispatchEvent(
      new ClipboardEvent("paste", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, text);
}

test("comments appear as anchored margin cards without a side panel", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.keyboard.type("COMMENT_TARGET");
  for (let i = 0; i < "COMMENT_TARGET".length; i++) {
    await page.keyboard.press("Shift+ArrowLeft");
  }
  await expect(page.locator("#selComment")).toBeVisible();
  await page.locator("#selComment").click();
  const composer = page.locator(".review-popover textarea");
  await expect(composer).toBeVisible();
  await composer.fill("Margin comment");
  await page.locator('[data-testid="review-comment-submit"]').click();

  const card = page.locator(".review-margin-card.review-margin-comment");
  await expect(card).toBeVisible();
  await expect(card).toContainText("Margin comment");
  await card.getByRole("button", { name: "Reply" }).click();
  await page.locator(".review-popover textarea").fill("Thread reply");
  await page.locator('[data-testid="review-comment-submit"]').click();
  await expect(card.locator(".review-margin-reply")).toContainText("Thread reply");
  await card.locator(".review-margin-reply").getByRole("button", { name: "Resolve" }).click();
  await expect(card.locator(".review-margin-reply")).toHaveClass(/resolved/);
  await card.locator(":scope > .review-margin-card-actions").getByRole("button", { name: "Resolve" }).click();
  await expect(card).toHaveClass(/resolved/);
  await card.locator(":scope > .review-margin-card-actions").getByRole("button", { name: "Reopen" }).click();
  await expect(card).not.toHaveClass(/resolved/);
  await expect(page.locator("#reviewPanel")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("suggesting paste appears beside the page and can be accepted", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.locator("#reviewInlineMode").click();
  await expect(page.locator("#reviewInlineMode")).toHaveText("Suggesting");
  await pastePlainText(page, "TRACKED_MARGIN_INSERT");

  const card = page.locator(".review-margin-card.review-margin-insertion");
  await expect(card).toBeVisible();
  await expect(card).toContainText("TRACKED_MARGIN_INSERT");
  await card.getByRole("button", { name: "Accept" }).click();
  await expect(card).toHaveCount(0);

  await pastePlainText(page, "TRACKED_MARGIN_REJECT");
  const rejected = page.locator(".review-margin-card.review-margin-insertion");
  await expect(rejected).toContainText("TRACKED_MARGIN_REJECT");
  await rejected.getByRole("button", { name: "Reject" }).click();
  await expect(rejected).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});
