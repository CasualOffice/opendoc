import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

const TRACKED_MOVE_DOCX = "UEsDBBQAAAAIABwY/1ydxYoq8gAAALkBAAATAAAAW0NvbnRlbnRfVHlwZXNdLnhtbH2QzU7DMBCE73kKy1eUOHBACCXpgZ8jcCgPsLI3iVV7bXnd0r49TgtFQpSjNfPNrKdb7b0TO0xsA/XyummlQNLBWJp6+b5+ru+k4AxkwAXCXh6Q5WqouvUhIosCE/dyzjneK8V6Rg/chIhUlDEkD7k806Qi6A1MqG7a9lbpQBkp13nJkEMlRPeII2xdFk/7opxuSehYioeTd6nrJcTorIZcdLUj86uo/ippCnn08GwjXxWDVJdKFvFyxw/6WiZK1qB4g5RfwBej+gjJKBP01he4+T/pj2vDOFqNZ35JiyloZC7be9ecFQ+Wvn/RqePwQ/UJUEsDBAoAAAAAABwY/1wAAAAAAAAAAAAAAAAGAAAAX3JlbHMvUEsDBBQAAAAIABwY/1xAoFMJsgAAAC8BAAALAAAAX3JlbHMvLnJlbHONz7sOgjAUBuCdp2jOLgUHYwyFxZiwGnyApj2URnpJWy+8vR0cxDg4ntt38jfd08zkjiFqZxnUZQUErXBSW8XgMpw2eyAxcSv57CwyWDBC1xbNGWee8k2ctI8kIzYymFLyB0qjmNDwWDqPNk9GFwxPuQyKei6uXCHdVtWOhk8D2oKQFUt6ySD0sgYyLB7/4d04aoFHJ24Gbfrx5WsjyzwoTAweLkgq3+0ys0BzSrqK2RYvUEsDBAoAAAAAABwY/1wAAAAAAAAAAAAAAAAFAAAAd29yZC9QSwMEFAAAAAgAHBj/XNCh0AF8AQAAzAMAABEAAAB3b3JkL2RvY3VtZW50LnhtbLVTPU/DMBDd+ysi7yGpKVBFTRADbEiIhoXNxNckUuyz7GtD+fU4aWhaaAeEkDzc8328u+fz4vZdNcEGrKtRp2x6EbMAdIGy1mXKXvKHcM4CR0JL0aCGlG3BsdtssmgTicVagabAV9AuaVNWEZkkilxRgRLuAg1o71uhVYI8tGXUopXGYgHOeQLVRDyOryMlas2ySRD4qm8ot53ZA7OzelvhBh4sqmehS1iSsBS0SS1T5jtuEy2Ub66LCS1samhDI2rbecSaKrQpu5Oig1KQD+Qxvw7jm/Bymsdx0p9XFp1gGzimv6v0VagvZbNOLGhyeKfMQoOFT5SLaLzsbLsnj0b2c+Pfa7kffui6SzNnhcvxh2z8v2TLcWC4/Kto3+Q6I1SOp4cdReKnRXJQ0JM9SDbl8sOn+E2ecj7r96ry9tV8Fh9NacpHYb2T0Hj3bBdp67KiEb4hEaoRN7A68FYgJHhVbngPV4h0AMs19fDoccduO7T7Jv0SDd8wm3wCUEsBAh4DFAAAAAgAHBj/XJ3FiiryAAAAuQEAABMAAAAAAAAAAQAAAKSBAAAAAFtDb250ZW50X1R5cGVzXS54bWxQSwECHgMKAAAAAAAcGP9cAAAAAAAAAAAAAAAABgAAAAAAAAAAABAA7UEjAQAAX3JlbHMvUEsBAh4DFAAAAAgAHBj/XECgUwmyAAAALwEAAAsAAAAAAAAAAQAAAKSBRwEAAF9yZWxzLy5yZWxzUEsBAh4DCgAAAAAAHBj/XAAAAAAAAAAAAAAAAAUAAAAAAAAAAAAQAO1BIgIAAHdvcmQvUEsBAh4DFAAAAAgAHBj/XNCh0AF8AQAAzAMAABEAAAAAAAAAAQAAAKSBRQIAAHdvcmQvZG9jdW1lbnQueG1sUEsFBgAAAAAFAAUAIAEAAPADAAAAAA==";

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
  await expect(card).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator(".review-comment-marker")).toHaveCount(0);
  await card.click();
  await expect(card).toHaveAttribute("aria-expanded", "true");
  await expect(page.locator(".review-comment-marker")).not.toHaveCount(0);
  await card.click();
  await expect(card).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator(".review-comment-marker")).toHaveCount(0);
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
  await expect(page.locator("#suggestingBanner")).toBeVisible();
  await pastePlainText(page, "TRACKED_SIDEBAR_INSERT");

  const sidebar = page.locator("#reviewSidebar");
  const card = sidebar.locator(".review-margin-card.review-margin-insertion");
  await expect(sidebar).toBeVisible();
  await expect(card).toBeVisible();
  await expect(card).toContainText("TRACKED_SIDEBAR_INSERT");
  await page.keyboard.press("Backspace");
  await expect(card.locator(".review-margin-body")).toHaveText("Added “TRACKED_SIDEBAR_INSER”");
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

test("replacement and formatting pairs are one atomic suggestion card", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("OLD");
  await page.keyboard.press("Shift+ArrowLeft");
  await page.keyboard.press("Shift+ArrowLeft");
  await page.keyboard.press("Shift+ArrowLeft");
  await page.locator("#reviewInlineMode").click();
  await page.locator("#alignCenter").click();
  await expect(page.locator("#status")).toContainText("cannot be tracked");
  await page.keyboard.type("NEW");

  const sidebar = page.locator("#reviewSidebar");
  const replacement = sidebar.locator(".review-margin-card.review-margin-replacement");
  await expect(replacement).toHaveCount(1);
  await expect(replacement).toContainText("Replaced “OLD” with “NEW”");
  await replacement.click();
  await replacement.getByRole("button", { name: "Accept" }).click();
  await expect(replacement).toHaveCount(0);

  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Shift+ArrowRight");
  await page.locator("#bold").click();
  const formatting = sidebar.locator(".review-margin-card.review-margin-formatting");
  await expect(formatting).toHaveCount(1);
  await expect(formatting).toContainText("Changed formatting for “NEW”");
  await formatting.click();
  await formatting.getByRole("button", { name: "Reject" }).click();
  await expect(formatting).toHaveCount(0);

  await page.locator("#suggestingBanner").getByRole("button", { name: "Switch to editing" }).click();
  await expect(page.locator("#reviewInlineMode")).toHaveText("Editing");
  await expect(page.locator("#suggestingBanner")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("an imported tracked move is one atomic source-to-destination card", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles({
    name: "tracked-move.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: Buffer.from(TRACKED_MOVE_DOCX, "base64"),
  });

  const sidebar = page.locator("#reviewSidebar");
  const move = sidebar.locator(".review-margin-card.review-margin-move");
  await expect(move).toHaveCount(1);
  await expect(move).toContainText("Moved “relocated”");
  await expect(move).toContainText("From");
  await expect(move).toContainText("Original location");
  await expect(move).toContainText("To");
  await expect(move).toContainText("New location");
  await expect(sidebar.locator(".review-margin-move-from")).toHaveCount(0);
  await expect(sidebar.locator(".review-margin-move-to")).toHaveCount(0);
  await expect(page.locator(".review-deletion-marker")).toHaveCount(1);
  await expect(page.locator(".review-insertion-marker")).toHaveCount(1);

  await move.click();
  await expect(page.locator(".review-revision-marker-active")).toHaveCount(2);
  await move.getByRole("button", { name: "Accept" }).click();
  await expect(move).toHaveCount(0);
  await expect(page.locator(".review-revision-marker")).toHaveCount(0);

  await page.locator("#undoBtn").click();
  await expect(move).toHaveCount(1);
  await expect(move.getByRole("button", { name: "Reject" })).toBeVisible();
  await move.getByRole("button", { name: "Reject" }).click();
  await expect(move).toHaveCount(0);
  await expect(page.locator(".review-revision-marker")).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});
