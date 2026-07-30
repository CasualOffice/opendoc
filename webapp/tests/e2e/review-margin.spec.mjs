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

async function expectSidebarBesideCanvas(page) {
  const geometry = await page.evaluate(() => {
    const canvas = document.querySelector(".page");
    const sidebar = document.querySelector("#reviewSidebar");
    const canvasRect = canvas.getBoundingClientRect();
    const sidebarRect = sidebar.getBoundingClientRect();
    return {
      canvasRight: canvasRect.right,
      sidebarLeft: sidebarRect.left,
      sidebarWidth: sidebarRect.width,
    };
  });
  expect(geometry.sidebarLeft).toBeGreaterThanOrEqual(geometry.canvasRight);
  expect(geometry.sidebarWidth).toBeGreaterThanOrEqual(300);
}

test("comments use the dedicated sidebar and an in-column composer", async ({
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
  await page.locator("#selComment").click();

  const sidebar = page.locator("#reviewSidebar");
  await expect(sidebar).toBeVisible();
  await expect(page.locator(".review-popover")).toHaveCount(0);
  await expectSidebarBesideCanvas(page);

  const composer = sidebar.locator('[data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.fill("Sidebar comment");
  await sidebar.locator('[data-testid="review-comment-submit"]').click();

  const card = sidebar.locator(".review-margin-card.review-margin-comment");
  await expect(card).toBeVisible();
  await expect(card).toContainText("Sidebar comment");
  await expect(card).toHaveAttribute("aria-expanded", "false");
  await card.click();
  await expect(card).toHaveAttribute("aria-expanded", "true");

  const reply = card.locator(".review-reply-composer input");
  await reply.click();
  await reply.fill("Thread reply");
  await card.locator(".review-reply-composer").getByRole("button", { name: "Reply" }).click();
  await expect(card.locator(".review-margin-reply")).toContainText("Thread reply");

  await card.locator(":scope > .review-margin-card-head").getByRole("button", { name: "Resolve" }).click();
  await expect(card).toHaveClass(/resolved/);
  await expect(page.locator("#reviewPanel")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("suggestions share the sidebar and decisions appear only on the active card", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await expect(page.locator("#reviewInlineBar")).toBeHidden();
  await page.locator("#reviewInlineMode").click();
  await expect(page.locator("#reviewInlineMode")).toHaveText("Suggesting");
  await pastePlainText(page, "TRACKED_SIDEBAR_INSERT");

  const sidebar = page.locator("#reviewSidebar");
  const card = sidebar.locator(".review-margin-card.review-margin-insertion");
  await expect(sidebar).toBeVisible();
  await expect(card).toBeVisible();
  await expect(card).toContainText("TRACKED_SIDEBAR_INSERT");
  await expect(card.getByRole("button", { name: "Accept" })).toBeHidden();
  await card.click();
  await expect(card.getByRole("button", { name: "Accept" })).toBeVisible();
  await expectSidebarBesideCanvas(page);
  await card.getByRole("button", { name: "Accept" }).click();
  await expect(card).toHaveCount(0);

  await pastePlainText(page, "TRACKED_SIDEBAR_REJECT");
  const rejected = sidebar.locator(".review-margin-card.review-margin-insertion");
  await rejected.click();
  await rejected.getByRole("button", { name: "Reject" }).click();
  await expect(rejected).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});
