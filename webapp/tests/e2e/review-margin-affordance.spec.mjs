// The right-margin comment affordance.
//
// With the comment column closed, the right of the work area was empty and
// offered nothing: the two durable entry points (the rail's Comments toggle and
// the Review band's Comments pane) are both on the far side of the window, and
// the floating selection toolbar's comment button lives over the text and
// disappears on the first scroll. So the one place a reader looks when they
// want to say something about a sentence — beside the sentence — was the only
// place in the chrome with no affordance at all.
//
// The reference is Google Docs: an Add-comment button in the page's right
// margin, on the line the caret is in. Word instead puts a Comments button in
// the window's top right and marks the text. ONLYOFFICE has no margin button at
// all (their panel-closed signals are a per-author highlight, a 2px in-text
// range mark, and a 7px dot on the rail button) — but their comment popover is
// anchored at the same point: `private_GetCommentWorldAnchorPoint` takes X from
// `Get_PageLimits(nPage).XLimit`, the right edge of the text area, and Y from
// the commented line. This chrome already renders its comment cards in that
// margin, so Docs' affordance is the one that belongs in it.
//
// These assertions are about the guarantee, not the mechanism: the button is
// beside the page rather than over it, it stays on its own line while the
// document scrolls, it opens the SAME composer every other surface opens, and
// it is declared as a face of `review.comment` rather than as a second command.
import { test, expect, gotoEditor, clickIntoFirstPage, stableBox } from "./fixtures.mjs";

const affordance = (page) => page.locator("#reviewMarginComment");

/** Selects `length` characters forward from the caret, making a real range. */
async function selectForward(page, length) {
  for (let i = 0; i < length; i++) await page.keyboard.press("Shift+ArrowRight");
}

/** The union of the painted selection highlight — the line the affordance is
 *  supposed to be beside. Read from the overlay the user can see, not from the
 *  engine, so a button aligned to the wrong rect cannot pass. */
async function selectionBox(page) {
  return page.evaluate(() => {
    const rects = [...document.querySelectorAll("#pages .overlay .highlight")];
    if (!rects.length) return null;
    const boxes = rects.map((el) => el.getBoundingClientRect());
    return {
      top: Math.min(...boxes.map((b) => b.top)),
      bottom: Math.max(...boxes.map((b) => b.bottom)),
    };
  });
}

const pageBox = (page) => stableBox(page.locator(".page-wrap .page").first());

test("with the column closed, a selection gets a comment button in the margin", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await expect(page.locator("#reviewSidebar")).toBeHidden();

  await clickIntoFirstPage(page);
  await selectForward(page, 8);

  const button = affordance(page);
  await expect(button).toBeVisible();
  await expect(button).toBeEnabled();

  const box = await stableBox(button);
  const sheet = await pageBox(page);
  const line = await selectionBox(page);
  expect(line).not.toBeNull();

  // Beside the page, never over it — the HF-088 rule, one control smaller.
  expect(box.x).toBeGreaterThanOrEqual(sheet.x + sheet.width);
  // ...and inside the work area rather than under the scrollbar or off-screen.
  const viewport = await stableBox(page.locator("#viewport"));
  expect(box.x + box.width).toBeLessThanOrEqual(viewport.x + viewport.width);

  // On the selection's own line: centres within half a line of each other.
  const buttonMid = box.y + box.height / 2;
  const lineMid = (line.top + line.bottom) / 2;
  expect(Math.abs(buttonMid - lineMid)).toBeLessThanOrEqual(8);

  expect(consoleErrors).toEqual([]);
});

test("it rides the document scroll instead of vanishing like the floating toolbar", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await selectForward(page, 8);
  await expect(affordance(page)).toBeVisible();

  const before = await stableBox(affordance(page));
  const lineBefore = await selectionBox(page);

  const moved = await page.locator("#viewport").evaluate((viewport) => {
    const from = viewport.scrollTop;
    viewport.scrollTop = from + 240;
    return viewport.scrollTop - from;
  });
  expect(moved).toBeGreaterThan(0);
  // Two frames: one for the scroll handler's rAF, one for it to paint.
  await page.evaluate(
    () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))),
  );

  // Still there (the mini toolbar hides on scroll; this must not), and still on
  // its line: it moved up by exactly what the text moved up by.
  await expect(affordance(page)).toBeVisible();
  const after = await stableBox(affordance(page));
  const lineAfter = await selectionBox(page);
  expect(Math.abs((before.y - after.y) - (lineBefore.top - lineAfter.top))).toBeLessThanOrEqual(2);

  expect(consoleErrors).toEqual([]);
});

test("it opens the one comment composer, and stands down once the column is open", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await selectForward(page, 8);

  await affordance(page).click();

  const composer = page.locator('#reviewSidebar [data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.fill("From the margin");
  await page.locator('#reviewSidebar [data-testid="review-comment-submit"]').click();
  await expect(
    page.locator("#reviewSidebar .review-margin-card.review-margin-comment"),
  ).toContainText("From the margin");

  // The column now owns the margin, so the affordance gets out of its way
  // rather than sitting on top of the cards it just produced.
  await expect(page.locator("#reviewSidebar")).toBeVisible();
  await expect(affordance(page)).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

test("it is a face of review.comment, not a second way to comment", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);

  // The declaration, readable from the DOM: main.js stamps `data-command` on
  // every button a REVIEW_SURFACE row names, so a margin button wired up on its
  // own — free to drift in what it runs and when it is available — would show
  // up here as a missing or different id (`105` UX-004).
  await expect(affordance(page)).toHaveAttribute("data-command", "review.comment");
  await expect(page.locator("#reviewCommentBtn")).toHaveAttribute(
    "data-command",
    "review.comment",
  );

  // And the shared enablement rule, which is the other half of "one
  // declaration": a caret is not a comment anchor, so both faces are dimmed
  // together and both light up together.
  await clickIntoFirstPage(page);
  await expect(affordance(page)).toBeVisible();
  await expect(affordance(page)).toBeDisabled();
  await expect(page.locator("#reviewCommentBtn")).toBeDisabled();

  await selectForward(page, 8);
  await expect(affordance(page)).toBeEnabled();
  await expect(page.locator("#reviewCommentBtn")).toBeEnabled();

  expect(consoleErrors).toEqual([]);
});

// The refusal, in a real browser and at real widths. Measured on the rich
// fixture (794px sheet, 55px rail): the right margin is -458px at 390, -148px
// at 700 — the sheet width — 26px at 900, and 88px at 1024. The button needs 56
// (12 + 32 + 12), so it appears somewhere between 900 and 1024 and nowhere
// below. That is the whole reason this is decided by measuring the margin
// rather than by a mode or a media query: a button in a margin that does not
// exist is a button over the text it points at, which is HF-088 again.
for (const [width, shown] of [[390, false], [900, false], [1024, true]]) {
  test(`at ${width}px the margin ${shown ? "holds" : "cannot hold"} the button`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    await selectForward(page, 8);

    if (shown) {
      await expect(affordance(page)).toBeVisible();
      const box = await stableBox(affordance(page));
      const sheet = await pageBox(page);
      expect(box.x).toBeGreaterThanOrEqual(sheet.x + sheet.width);
    } else {
      await expect(affordance(page)).toBeHidden();
    }

    expect(consoleErrors).toEqual([]);
  });
}
