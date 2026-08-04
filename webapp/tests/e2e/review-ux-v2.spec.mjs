// Review UX v2 (docs/63, 68, 93): the industry-standard review interactions —
// redline visible-by-default bound to mode (Q1), an inline accept/reject card on
// a tracked-change marker (Q2), single-change keyboard Accept ▸ Next (Q3), a
// unified Next/Previous over comments AND changes (Q4), and an always-ready
// multi-line reply composer (Q5).
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

async function pastePlainText(page, text) {
  await page.evaluate((value) => {
    const data = new DataTransfer();
    data.setData("text/plain", value);
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, text);
}

/** Comment "body" over the word `target` typed at the current caret. */
async function addComment(page, target, body) {
  await page.keyboard.type(target);
  for (let i = 0; i < target.length; i++) await page.keyboard.press("Shift+ArrowLeft");
  await page.locator("#selComment").click();
  const sidebar = page.locator("#reviewSidebar");
  await sidebar.locator('[data-testid="review-comment-composer"]').fill(body);
  await sidebar.locator('[data-testid="review-comment-submit"]').click();
}

// Q1 — redline visible by default, bound to mode -----------------------------
test("entering Suggesting turns the markup (redline) view on automatically (Q1)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // The clean demo opens with markup off; entering Suggesting enables it.
  await expect(page.locator("body")).not.toHaveClass(/showing-changes/);
  await setReviewMode(page, "suggesting");
  await expect(page.locator("body")).toHaveClass(/showing-changes/);

  // A tracked deletion is then rendered as a visible struck marker without any
  // extra toggle — the whole point of Q1 (deletions no longer invisible).
  await moveCaretToDocStart(page);
  await pastePlainText(page, "REDLINE_DELETE_ME ");
  for (let i = 0; i < "REDLINE_DELETE_ME ".length; i++) await page.keyboard.press("Shift+ArrowLeft");
  await expect(page.locator(".review-insertion-marker").first()).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

// Q2 — inline accept/reject on a change --------------------------------------
test("hovering a tracked-change marker shows an inline card that accepts the change (Q2)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "INLINE_ACCEPT ");

  const sidebar = page.locator("#reviewSidebar");
  await expect(sidebar.locator(".review-margin-insertion")).toHaveCount(1);

  // Hover the change's marker on the canvas — the compact inline card appears
  // with the author, a summary, and Accept / Reject.
  const marker = page.locator(".overlay .review-insertion-marker").first();
  await marker.hover();
  const card = page.locator(".review-inline-card");
  await expect(card).toBeVisible();
  await expect(card).toContainText("Added");

  // ✔ Accept resolves that single change: its sidebar card and marker are gone.
  await card.getByRole("button", { name: /Accept/ }).click();
  await expect(page.locator(".review-inline-card")).toHaveCount(0);
  await expect(sidebar.locator(".review-margin-insertion")).toHaveCount(0);
  await expect(page.locator(".overlay .review-insertion-marker")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

// Q3 — Accept ▸ Next -----------------------------------------------------------
test("Accept and move to Next accepts the caret's change and advances (Q3)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // A plain prefix so neither tracked insertion sits at document offset 0.
  await page.keyboard.type("PREFIX ");
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "AONE ");
  await pastePlainText(page, "ATWO ");

  const sidebar = page.locator("#reviewSidebar");
  const insertions = sidebar.locator(".review-margin-insertion");
  await expect(insertions).toHaveCount(2);

  // Land the caret on the first change via unified Next, then Accept ▸ Next.
  await moveCaretToDocStart(page);
  await page.locator("#reviewNext").click();
  await expect(insertions.filter({ hasText: "AONE" })).toHaveAttribute("aria-expanded", "true");

  await page.keyboard.press(`${MOD}+Alt+Enter`);

  // The first change is accepted; the caret advanced to the second, whose card
  // is now the expanded one.
  await expect(insertions).toHaveCount(1);
  const remaining = sidebar.locator(".review-margin-insertion");
  await expect(remaining).toContainText("ATWO");
  await expect(remaining).toHaveAttribute("aria-expanded", "true");

  expect(consoleErrors).toEqual([]);
});

// Q4 — unified Next/Previous over comments AND changes ------------------------
test("Next/Previous traverse a document-ordered mix of a comment and a change (Q4)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // A comment at document start and a tracked change at document end — two
  // non-overlapping review items far apart, so Next/Previous visit each as its
  // own stop, spanning both item kinds.
  await addComment(page, "QNAVCOMMENT", "a threaded note");
  await clickIntoFirstPage(page); // re-focus the editor after the composer
  await page.keyboard.press(`${MOD}+End`); // caret to document end
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "ENDCHANGE ");

  const sidebar = page.locator("#reviewSidebar");
  const comment = sidebar.locator(".review-margin-comment");
  const change = sidebar.locator(".review-margin-insertion");
  await expect(comment).toHaveCount(1);
  await expect(change).toHaveCount(1);

  // From document start, Next visits the comment first, then the change — one
  // merged, document-ordered loop over both kinds (Q4).
  await moveCaretToDocStart(page);
  await page.locator("#reviewNext").click();
  await expect(comment).toHaveAttribute("aria-expanded", "true");

  await page.locator("#reviewNext").click();
  await expect(change).toHaveAttribute("aria-expanded", "true");
  await expect(comment).toHaveAttribute("aria-expanded", "false");

  // Previous walks back to the comment, proving the loop is bidirectional over
  // both item kinds.
  await page.locator("#reviewPrevious").click();
  await expect(comment).toHaveAttribute("aria-expanded", "true");

  expect(consoleErrors).toEqual([]);
});

// Q5 — always-ready multi-line reply composer --------------------------------
test("the reply composer is an always-ready multi-line textarea (Enter submits, Shift+Enter newlines) (Q5)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await addComment(page, "REPLY_ROOT", "Root comment");

  const sidebar = page.locator("#reviewSidebar");
  const card = sidebar.locator(".review-margin-card.review-margin-comment");
  await card.click();
  await expect(card).toHaveAttribute("aria-expanded", "true");

  // Always ready: a textarea (multi-line), typeable without a click-to-arm step.
  const reply = card.locator(".review-reply-composer textarea");
  await expect(reply).toHaveJSProperty("tagName", "TEXTAREA");
  await reply.focus();
  await page.keyboard.type("First line");
  await page.keyboard.press("Shift+Enter"); // newline, not submit
  await page.keyboard.type("Second line");
  await expect(reply).toHaveValue("First line\nSecond line");

  await page.keyboard.press("Enter"); // submit
  const replyItem = card.locator(".review-margin-reply");
  await expect(replyItem).toContainText("First line");
  await expect(replyItem).toContainText("Second line");
  // The composer is cleared and ready for the next reply.
  await expect(reply).toHaveValue("");

  expect(consoleErrors).toEqual([]);
});
