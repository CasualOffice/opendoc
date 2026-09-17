// docs/108 phase 2 — authoring paragraph-level suggestions (docs/104 HF-130, HF-131).
//
// Suggesting mode refused Enter, every cross-paragraph deletion and every
// paragraph formatting command: 17 "cannot be tracked yet" messages. A reviewer
// could not split a paragraph, join two, delete across a boundary, or recentre a
// heading without leaving Suggesting — which silently makes the edit untracked.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

/** The review sidebar's cards, by paragraph-level kind. */
const cards = (page, kind) =>
  page.locator(`#reviewSidebar .review-margin-card.review-margin-${kind}`);

/** Searches the document through the editor's own Find, which reads the model,
 *  and returns its status line ("1 match", "No match", …). Find concatenates the
 *  document's text, so it proves what a merge JOINED, not what a split
 *  separated; `paragraphCount` is the oracle for structure. The accessibility
 *  mirror is not an oracle at all here: it projects the opened document and does
 *  not follow text typed during the test. */
async function paragraphCount(page) {
  // The status bar's count is computed from the model on every edit.
  const text = await page.locator("#statParas").textContent();
  return Number(text.replace(/[^0-9]/g, ""));
}

async function find(page, query) {
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(query);
  const status = await page.locator("#findStatus").textContent();
  await page.keyboard.press("Escape");
  return status;
}

async function typeTwoParagraphs(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("ALPHA");
  await page.keyboard.press("Enter");
  await page.keyboard.type("BETA");
  await setReviewMode(page, "suggesting");
}

test("Enter while suggesting is a tracked paragraph break, not a refusal", async ({
  page,
  consoleErrors,
}) => {
  await typeTwoParagraphs(page);
  const before = await paragraphCount(page);

  await page.keyboard.press("ArrowLeft"); // inside BETA, before the last character
  await page.keyboard.press("Enter");

  await expect(page.locator("#status")).not.toContainText("cannot be tracked");
  await expect(cards(page, "paragraph-mark-insertion")).toHaveCount(1);
  await expect(page.locator(".review-paragraph-mark-inserted")).toHaveCount(1);
  expect(await paragraphCount(page), "the split is real").toBe(before + 1);

  // Rejecting it puts the paragraph back together.
  const card = cards(page, "paragraph-mark-insertion");
  await card.click();
  await card.getByRole("button", { name: "Reject" }).click();
  await expect(card).toHaveCount(0);
  expect(await paragraphCount(page), "rejecting joins the halves back").toBe(before);

  expect(consoleErrors).toEqual([]);
});

test("Backspace at a paragraph start suggests deleting that break", async ({
  page,
  consoleErrors,
}) => {
  await typeTwoParagraphs(page);
  const before = await paragraphCount(page);

  // Caret to the start of BETA, then Backspace.
  await page.keyboard.press("Home");
  await page.keyboard.press("Backspace");

  await expect(page.locator("#status")).not.toContainText("cannot be tracked");
  await expect(cards(page, "paragraph-mark-deletion")).toHaveCount(1);
  await expect(page.locator(".review-paragraph-mark-deleted")).toHaveCount(1);
  expect(await paragraphCount(page), "nothing merges until it is accepted").toBe(before);

  const card = cards(page, "paragraph-mark-deletion");
  await card.click();
  await card.getByRole("button", { name: "Accept" }).click();
  await expect(card).toHaveCount(0);
  // Accepting merges them: one paragraph fewer, and the two words are now one
  // paragraph's text.
  expect(await paragraphCount(page)).toBe(before - 1);
  expect(await find(page, "ALPHABETA")).toContain("1 match");

  expect(consoleErrors).toEqual([]);
});

test("a deletion across paragraphs is one tracked suggestion", async ({
  page,
  consoleErrors,
}) => {
  await typeTwoParagraphs(page);

  const before = await paragraphCount(page);

  // Select from inside ALPHA to inside BETA and delete.
  await moveCaretToDocStart(page);
  for (let i = 0; i < 2; i++) await page.keyboard.press("ArrowRight");
  for (let i = 0; i < 6; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Backspace");

  await expect(page.locator("#status")).not.toContainText("cannot be tracked");
  await expect(cards(page, "deletion")).not.toHaveCount(0);
  await expect(cards(page, "paragraph-mark-deletion")).toHaveCount(1);
  expect(await paragraphCount(page), "still separate until accepted").toBe(before);

  // One undo step takes the whole suggestion back.
  await page.locator("#undoBtn").click();
  await expect(page.locator("#reviewSidebar .review-margin-card")).toHaveCount(0);
  expect(await paragraphCount(page)).toBe(before);
  expect(await find(page, "ALPHA")).toContain("1 match");

  expect(consoleErrors).toEqual([]);
});

test("paragraph formatting while suggesting is tracked, and reject restores it", async ({
  page,
  consoleErrors,
}) => {
  await typeTwoParagraphs(page);

  await page.locator("#alignCenter").click();
  await expect(page.locator("#status")).not.toContainText("cannot be tracked");
  const card = cards(page, "paragraph-format");
  await expect(card).toHaveCount(1);
  await expect(card).toContainText(/Alignment: .+ → Centered/);
  await expect(page.locator(".review-paragraph-format-bar")).toHaveCount(1);

  // A second change on the same paragraph keeps the FIRST prior, so rejecting
  // returns it to where review started rather than to an intermediate state.
  await page.locator("#alignEnd").click();
  await expect(card).toHaveCount(1, { timeout: 10_000 });

  await card.click();
  await card.getByRole("button", { name: "Reject" }).click();
  await expect(card).toHaveCount(0);
  await expect(page.locator(".review-paragraph-format-bar")).toHaveCount(0);
  await expect(page.locator("#alignStart")).toHaveAttribute("aria-pressed", "true");

  expect(consoleErrors).toEqual([]);
});

test("leaving suggesting stops tracking paragraph formatting", async ({
  page,
  consoleErrors,
}) => {
  await typeTwoParagraphs(page);
  await setReviewMode(page, "editing");

  await page.locator("#alignCenter").click();
  await expect(page.locator("#alignCenter")).toHaveAttribute("aria-pressed", "true");
  await expect(cards(page, "paragraph-format")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});
