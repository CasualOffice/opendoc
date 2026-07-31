import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  MOD,
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

test("opening an empty comments sidebar preserves document scroll", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await expect(page.locator("#reviewSidebar")).toBeHidden();
  const before = await page.locator("#viewport").evaluate((viewport) => {
    viewport.scrollTop = Math.min(700, viewport.scrollHeight - viewport.clientHeight);
    return viewport.scrollTop;
  });
  expect(before).toBeGreaterThan(0);

  await page.locator("#railReview").click();
  await expect(page.locator("#reviewSidebar")).toBeVisible();
  await expect(page.locator(".review-sidebar-empty")).toContainText(
    "No comments or suggestions yet",
  );
  await page.evaluate(() => new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(resolve));
  }));
  const after = await page.locator("#viewport").evaluate((viewport) => viewport.scrollTop);
  expect(Math.abs(after - before)).toBeLessThanOrEqual(1);
  expect(consoleErrors).toEqual([]);
});

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

  // Show resolved comments so the card stays visible through the resolve flow
  // (the default "Open" filter would hide it once resolved — REVIEW-GAP-018/019).
  await sidebar.locator('[data-review-filter="all"]').click();
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
  expect(consoleErrors).toEqual([]);
});

test("a reply can be edited and individually deleted (REVIEW-GAP-011)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.keyboard.type("REPLY_TARGET");
  for (let i = 0; i < "REPLY_TARGET".length; i++) {
    await page.keyboard.press("Shift+ArrowLeft");
  }
  await page.locator("#selComment").click();

  const sidebar = page.locator("#reviewSidebar");
  const composer = sidebar.locator('[data-testid="review-comment-composer"]');
  await composer.fill("Root comment");
  await sidebar.locator('[data-testid="review-comment-submit"]').click();

  const card = sidebar.locator(".review-margin-card.review-margin-comment");
  await card.click();
  await expect(card).toHaveAttribute("aria-expanded", "true");

  const reply = card.locator(".review-reply-composer input");
  await reply.click();
  await reply.fill("First draft");
  await card.locator(".review-reply-composer").getByRole("button", { name: "Reply" }).click();
  const replyItem = card.locator(".review-margin-reply");
  await expect(replyItem).toContainText("First draft");

  // Edit the reply's text in place.
  await replyItem.getByRole("button", { name: "Edit" }).click();
  const replyEdit = card.locator(".review-margin-reply-edit");
  await replyEdit.fill("Edited reply text");
  await card.getByRole("button", { name: "Save" }).click();
  await expect(card.locator(".review-margin-reply")).toContainText("Edited reply text");
  await expect(card.locator(".review-margin-reply")).not.toContainText("First draft");

  // Delete just the reply; the root comment card survives.
  await card.locator(".review-margin-reply").getByRole("button", { name: "Delete" }).click();
  await expect(card.locator(".review-margin-reply")).toHaveCount(0);
  await expect(card).toContainText("Root comment");

  expect(consoleErrors).toEqual([]);
});

test("suggestions share the sidebar and decisions appear only on the active card", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await setReviewMode(page, "suggesting");
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
  await setReviewMode(page, "suggesting");
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
  await expect(formatting).toContainText(/Bold: (inherited|off|on) → (inherited|off|on)/);
  await formatting.click();
  await formatting.getByRole("button", { name: "Reject" }).click();
  await expect(formatting).toHaveCount(0);

  await page.locator("#suggestingBanner").getByRole("button", { name: "Switch to editing" }).click();
  await expect(
    page.locator('#reviewModeControl [data-review-mode="editing"]'),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#suggestingBanner")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("a standalone deletion stays visible at its collapsed document anchor", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("DELETE_ME");
  for (let index = 0; index < "DELETE_ME".length; index++) {
    await page.keyboard.press("Shift+ArrowLeft");
  }
  await setReviewMode(page, "suggesting");
  await page.keyboard.press("Backspace");

  const sidebar = page.locator("#reviewSidebar");
  const deletion = sidebar.locator(".review-margin-card.review-margin-deletion");
  await expect(deletion).toHaveCount(1);
  await expect(deletion).toContainText("Deleted “DELETE_ME”");
  await expect(page.locator(".review-deletion-marker")).toHaveCount(1);
  await deletion.click();
  await deletion.getByRole("button", { name: "Reject" }).click();
  await expect(deletion).toHaveCount(0);
  await expect(page.locator(".review-deletion-marker")).toHaveCount(0);
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
  // REVIEW-GAP-016: both ends of the move are keyboard-accessible navigation
  // buttons that name a precise location (the page each end sits on) rather
  // than a static generic label.
  const sourceNav = move.getByRole("button", { name: /original location/i });
  const destNav = move.getByRole("button", { name: /new location/i });
  await expect(sourceNav).toBeVisible();
  await expect(destNav).toBeVisible();
  await expect(sourceNav).toContainText("From");
  await expect(sourceNav).toContainText(/Page \d+/);
  await expect(destNav).toContainText("To");
  await expect(destNav).toContainText(/Page \d+/);
  await expect(sidebar.locator(".review-margin-move-from")).toHaveCount(0);
  await expect(sidebar.locator(".review-margin-move-to")).toHaveCount(0);
  await expect(page.locator(".review-deletion-marker")).toHaveCount(1);
  await expect(page.locator(".review-insertion-marker")).toHaveCount(1);

  // Activating a move-end nav button jumps to that end and does not toggle the
  // card's own expansion (the inner button owns its keyboard). The destination
  // ("relocated" at its new spot) is a real range, so navigating there selects
  // it; the move source is zero-width, so navigating there collapses to a caret
  // — proving both ends resolve to their own distinct anchors.
  await destNav.click();
  await expect(page.locator(".overlay .highlight")).not.toHaveCount(0);
  await sourceNav.click();
  await expect(page.locator(".overlay .highlight")).toHaveCount(0);
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

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

// REVIEW-GAP-005 (docs/81): the comment highlight span used to stop pointer
// propagation and reset the selection to the *entire* comment range on any
// click inside it, hijacking ordinary caret placement. Document hit-testing
// must remain authoritative — clicking inside commented text should place
// the caret exactly where the user clicked, the same as clicking anywhere
// else. Card expansion is allowed as a non-blocking secondary effect only.
test("clicking inside a commented range places the caret at the click position", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const marker = "COMMENT_HIT_TARGET_WORD";
  await page.keyboard.type(marker);
  await moveCaretToDocStart(page);
  for (let i = 0; i < marker.length; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.locator("#selComment").click();

  const sidebar = page.locator("#reviewSidebar");
  const composer = sidebar.locator('[data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.fill("hit-test comment");
  await sidebar.locator('[data-testid="review-comment-submit"]').click();

  const card = sidebar.locator(".review-margin-card.review-margin-comment");
  await expect(card).toBeVisible();
  await expect(card).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator(".review-comment-marker")).not.toHaveCount(0);

  // Submitting the composer left keyboard focus on the sidebar button, not
  // the editable surface — refocus it directly (bypassing any click on the
  // canvas, so this setup step cannot itself land inside the comment and
  // pre-expand the card before the real test click below).
  await page.evaluate(() => document.getElementById("pages").focus({ preventScroll: true }));

  // Ground truth: derive the exact overlay pixel position of an offset well
  // inside the commented word using ordinary keyboard caret navigation,
  // which is never routed through the comment-highlight click path.
  const targetOffset = 8; // inside "COMMENT_H|IT_TARGET_WORD" — not an edge
  await moveCaretToDocStart(page);
  for (let i = 0; i < targetOffset; i++) await page.keyboard.press("ArrowRight");
  const expectedCaret = await page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    return caret && {
      left: caret.style.left,
      top: caret.style.top,
      height: caret.style.height,
    };
  });
  expect(expectedCaret).toBeTruthy();

  // Move the caret far away so the upcoming click is the only thing that can
  // put it back at the target offset.
  await page.keyboard.press(`${MOD}+End`);
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  const awayCaret = await page.evaluate(() => document.querySelector(".overlay .caret").style.left);
  expect(awayCaret).not.toBe(expectedCaret.left);

  // Click on the commented highlight itself, at the same pixel position as
  // the recorded caret. Before the fix, this click landed on the
  // `.review-comment-marker` overlay, stopped propagation, and replaced the
  // selection with the *entire* comment range instead of a caret here.
  //
  // Dispatched via raw mouse coordinates (not a locator + `{position}` click):
  // Playwright's locator-click actionability check requires the *target
  // locator itself* to be the top hit-tested element, and it deliberately is
  // not — `.review-comment-marker` (pointer-events: auto) sits on top of the
  // canvas and is the real click target, exactly like a genuine user click
  // on commented text. The fix relies on that pointerdown bubbling up to the
  // page's own hit-testing, which `page.mouse.click` exercises faithfully.
  const canvasBox = await page.locator(".page-wrap .page").first().boundingBox();
  const clickPosition = await page.evaluate(({ left, top, height }) => ({
    x: Math.round(Number.parseFloat(left)) + 1,
    y: Math.round(Number.parseFloat(top)) + Math.max(2, Math.round(Number.parseFloat(height) / 2)),
  }), expectedCaret);
  const target = { x: canvasBox.x + clickPosition.x, y: canvasBox.y + clickPosition.y };
  expect(
    await page.evaluate(({ x, y }) => document.elementFromPoint(x, y)?.className, target),
  ).toContain("review-comment-marker");
  await page.mouse.click(target.x, target.y);

  const actualCaret = await page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    return caret && { left: caret.style.left, top: caret.style.top };
  });
  expect(actualCaret).toEqual({ left: expectedCaret.left, top: expectedCaret.top });
  // A collapsed caret, not a full-range selection highlight, must be shown.
  await expect(page.locator(".overlay .highlight")).toHaveCount(0);

  // Non-blocking secondary effect: the comment card is still surfaced so the
  // user can still find/open the comment from the sidebar.
  await expect(card).toHaveAttribute("aria-expanded", "true");

  expect(consoleErrors).toEqual([]);
});
