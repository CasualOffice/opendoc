// The review shell (PROPOSAL D-11, `docs/109` UX-065; audit REVIEW-01/04/05/06,
// REVIEW-10/11/12, REVIEW-V-02, DIALOGS-03).
//
// Each test creates its condition and asserts the guarantee a person sees:
//
// * cards never cover the text column or each other, and each sits beside its
//   anchor (the selected item's highlight on the page);
// * a card scrolling up passes UNDER the comments bar and never paints above it
//   (`elementFromPoint` across the bar finds the bar, not a card);
// * Accept all / Reject all are absent with nothing to decide, and present (in
//   the bar's ⋯) once a suggestion exists;
// * typed text in a new comment survives Escape as a Draft;
// * Escape on an open card collapses it and keeps focus on that card; the next
//   Escape closes the column and focus goes to its rail button — never <body>;
// * a comment is shown in its own direction, with its web address as a link.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

async function settle(page) {
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
}

/** Adds a paragraph at the end of the document and comments on it. */
async function commentNewLine(page, line, note) {
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
  await page.keyboard.insertText(line);
  await page.keyboard.press("Shift+Home");
  await page.locator("#selComment").click();
  const composer = page.getByTestId("review-comment-composer");
  await expect(composer).toBeVisible();
  await composer.fill(note);
  await page.getByTestId("review-comment-submit").click();
  await expect(composer).toHaveCount(0);
}

const intersects = (a, b) => a.left < b.right - 1 && b.left < a.right - 1 && a.top < b.bottom - 1 && b.top < a.bottom - 1;

test("cards sit beside their anchors, clear of the text column and of each other", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await commentNewLine(page, "First commented line of text", "One");
  for (let i = 0; i < 6; i++) await page.keyboard.press("Enter");
  await commentNewLine(page, "Second commented line of text", "Two");
  const cards = page.locator("#reviewSidebar .review-margin-card.review-margin-comment");
  await expect(cards).toHaveCount(2);
  await settle(page);
  const rects = await page.evaluate(() => {
    const box = (el) => { const r = el.getBoundingClientRect(); return { left: r.left, right: r.right, top: r.top, bottom: r.bottom }; };
    return {
      page: box(document.querySelector(".page-wrap")),
      cards: [...document.querySelectorAll("#reviewSidebar .review-margin-card")].map(box),
    };
  });
  for (const card of rects.cards) expect(intersects(card, rects.page), `a card covers the page: ${JSON.stringify(card)}`).toBe(false);
  expect(intersects(rects.cards[0], rects.cards[1]), "two cards overlap").toBe(false);
  // Beside its anchor: select each card, and its top is level with its own
  // highlight on the page (the review-scroll-sync tolerance).
  for (let i = 0; i < 2; i++) {
    await cards.nth(i).click();
    await settle(page);
    const gap = await page.evaluate((index) => {
      const card = document.querySelectorAll("#reviewSidebar .review-margin-card.review-margin-comment")[index];
      const marks = [...document.querySelectorAll(".overlay .highlight")];
      if (!card || !marks.length) return null;
      return Math.abs(card.getBoundingClientRect().top - Math.min(...marks.map((m) => m.getBoundingClientRect().top)));
    }, i);
    expect(gap, `card ${i} has no anchor highlight`).not.toBeNull();
    expect(gap, `card ${i} is ${gap}px from its anchor`).toBeLessThanOrEqual(28);
  }
  expect(consoleErrors).toEqual([]);
});

test("a card scrolling up passes under the comments bar and never shows above it", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await commentNewLine(page, "A commented line near the top", "Scroll me under the bar");
  const card = page.locator("#reviewSidebar .review-margin-card.review-margin-comment");
  await expect(card).toHaveCount(1);
  // Scroll so the card straddles the bar.
  const delta = await page.evaluate(() => {
    const bar = document.getElementById("reviewSidebarHeader").getBoundingClientRect();
    const c = document.querySelector("#reviewSidebar .review-margin-card").getBoundingClientRect();
    return c.top - bar.top - 4;
  });
  await page.locator("#viewport").evaluate((el, by) => { el.scrollTop += by; }, delta);
  await settle(page);
  const probe = await page.evaluate(() => {
    const bar = document.getElementById("reviewSidebarHeader");
    const r = bar.getBoundingClientRect();
    const viewportTop = document.getElementById("viewport").getBoundingClientRect().top;
    const c = document.querySelector("#reviewSidebar .review-margin-card").getBoundingClientRect();
    const xs = [0.1, 0.3, 0.5, 0.7, 0.9].map((f) => r.left + r.width * f);
    // Every row from the scroller's top edge down to the bar's bottom: the
    // bar's own rows, and any strip left between the scroller and the bar.
    const ys = [viewportTop + 1, r.top + 1, r.top + r.height / 2, r.bottom - 2];
    const clear = (y) => xs.every((x) => !document.elementFromPoint(x, y)?.closest(".review-margin-card"));
    // Hit-testing alone cannot see paint: a transparent bar is "on top" of the
    // card and still shows it. The bar must also be opaque.
    const alpha = (() => {
      const m = getComputedStyle(bar).backgroundColor.match(/rgba?\(([^)]+)\)/);
      if (!m) return 0;
      const parts = m[1].split(/[ ,/]+/).filter(Boolean);
      return parts.length > 3 ? Number(parts[3]) : 1;
    })();
    return {
      straddles: c.top < r.bottom && c.bottom > r.bottom,
      opaque: alpha === 1,
      barOnTop: ys.slice(1).every(clear),
      // From the scroller's top edge (the ribbon band's lower edge): no card.
      aboveClear: clear(ys[0]) && xs.every((x) => !document.elementFromPoint(x, viewportTop - 2)?.closest(".review-margin-card")),
    };
  });
  expect(probe.straddles, "the condition: the card straddles the bar's bottom edge").toBe(true);
  expect(probe.opaque, "the comments bar is see-through, so a card under it shows").toBe(true);
  expect(probe.barOnTop, "a card painted over the comments bar").toBe(true);
  expect(probe.aboveClear, "a card escaped above the comment column").toBe(true);
});

test("Accept all and Reject all are absent with nothing to decide, and in the ⋯ menu once a suggestion exists", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator("#railReview").click();
  await page.locator("#reviewMoreBtn").click();
  await expect(page.locator("#reviewMoreMenu")).toBeVisible();
  await expect(page.locator("#reviewNext")).toBeVisible();
  await expect(page.locator("#reviewAcceptAll")).toBeHidden();
  await expect(page.locator("#reviewRejectAll")).toBeHidden();
  await page.keyboard.press("Escape");
  await expect(page.locator("#reviewMoreMenu")).toBeHidden();
  // A suggestion.
  await setReviewMode(page, "suggesting");
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.insertText(" SUGGESTED");
  await expect(page.locator("#reviewSidebar .review-margin-revision").first()).toBeVisible();
  await expect(page.locator("#reviewOpenCount")).toBeVisible();
  // The suggestion card carries Accept and Reject itself, collapsed.
  await expect(page.locator("#reviewSidebar .review-margin-revision").first().getByRole("button", { name: /^Accept/ })).toBeVisible();
  await page.locator("#reviewMoreBtn").click();
  await expect(page.locator("#reviewAcceptAll")).toBeVisible();
  await page.locator("#reviewAcceptAll").click();
  await expect(page.locator("#reviewMoreMenu")).toBeHidden();
  // Said once, with Undo beside it; focus is back in the document, not <body>.
  await expect(page.locator("#undoSnackbar")).toBeVisible();
  await expect(page.locator("#reviewSidebar .review-margin-revision")).toHaveCount(0);
  const onBody = await page.evaluate(() => document.activeElement === document.body);
  expect(onBody, "focus fell to <body> after Accept all").toBe(false);
  await page.locator("#undoSnackbarUndo").click();
  await expect(page.locator("#reviewSidebar .review-margin-revision").first()).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("Suggesting is the chip and a rule, not a banner: the page moves 0px", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const top = () => page.locator(".page-wrap").first().evaluate((el) => el.getBoundingClientRect().top);
  const before = await top();
  await setReviewMode(page, "suggesting");
  await expect(page.locator("#headerModeBtn")).toHaveAttribute("data-mode", "suggesting");
  await settle(page);
  expect(await top(), "entering Suggesting moved the page").toBe(before);
  await expect(page.locator("#suggestingBanner")).toBeHidden();
  // Exactly one mode surface on screen: the header chip.
  const surfaces = await page.evaluate(() =>
    ["#headerModeBtn", "#reviewModeControl", "#suggestingBanner"].filter((s) => {
      const el = document.querySelector(s);
      return el && !el.hidden && el.getClientRects().length > 0;
    }),
  );
  expect(surfaces).toEqual(["#headerModeBtn"]);
  expect(consoleErrors).toEqual([]);
});

test("typed text in a new comment survives Escape as a Draft", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
  await page.keyboard.insertText("Draft anchor line");
  await page.keyboard.press("Shift+Home");
  await page.locator("#selComment").click();
  const composer = page.getByTestId("review-comment-composer");
  await composer.fill("half a thought");
  await composer.press("Escape");
  await settle(page);
  await expect(composer).toHaveValue("half a thought");
  await expect(page.locator(".review-margin-composer.is-draft")).toBeVisible();
  // An empty composer still closes on Escape.
  await page.getByTestId("review-comment-cancel").click();
  await expect(composer).toHaveCount(0);
});

test("Escape on an open card collapses it with focus on the card; the next Escape closes the column onto its rail button", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await commentNewLine(page, "Escape anchor line", "Escape me");
  const card = page.locator("#reviewSidebar .review-margin-card.review-margin-comment");
  await card.click();
  await expect(card).toHaveAttribute("aria-expanded", "true");
  await card.focus();
  await page.keyboard.press("Escape");
  await expect(card).toHaveAttribute("aria-expanded", "false");
  await expect(card).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.locator("#reviewSidebar")).toBeHidden();
  await expect(page.locator("#railReview")).toBeFocused();
});

test("a comment runs in its own direction and its web address is a link", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await commentNewLine(page, "Direction anchor line", "مرحبا هذا تعليق https://example.com/x.");
  const body = page.locator("#reviewSidebar .review-margin-comment .review-margin-body").first();
  await expect(body).toHaveAttribute("dir", "auto");
  expect(await body.evaluate((el) => getComputedStyle(el).direction)).toBe("rtl");
  const link = body.locator("a");
  await expect(link).toHaveAttribute("href", "https://example.com/x");
  await expect(link).toHaveAttribute("rel", "noopener noreferrer");
});
