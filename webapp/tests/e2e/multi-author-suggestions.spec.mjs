// A second reviewer could not touch the first reviewer's pending suggestion.
// Typing inside it, or selecting it and pressing Backspace, did nothing at all —
// only "That edit isn't supported for this selection yet" flashed in the status
// line. Nothing the second reviewer typed existed anywhere.
//
// The cause was one refusal: `review_split_top_level_run` walked only top-level
// runs, so an offset landing inside a `Revision` wrapper returned false and the
// whole edit was rejected. docs/86 already specified the answer — another
// author's suggestion is "left intact and… marked with our own deletion at the
// boundary; we do not silently discard another author's suggestion" — so the
// wrapper is SPLIT (dividing the range without touching a character of its
// content) and a deletion inside one NESTS, exactly as Word's `w:del` inside
// `w:ins` does.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

async function reviewAs(page, author) {
  await page.locator("#settingsBtn").click();
  await page.locator("#authorName").fill(author);
  await page.locator("#authorName").press("Enter");
  await page.keyboard.press("Escape");
  await expect(page.locator("#settingsPanel, #settingsDialog").first()).toBeHidden();
}

const documentText = (page) =>
  page.locator("#a11yDocument").evaluate((el) => el.textContent.slice(0, 40));

/** Ada suggests "ADAWORD" at the very start of the document. */
async function adaSuggests(page) {
  await reviewAs(page, "Ada");
  await clickIntoFirstPage(page);
  await page.locator('#reviewModeControl [data-review-mode="suggesting"]').click();
  await moveCaretToDocStart(page);
  await page.keyboard.type("ADAWORD");
  await expect.poll(() => documentText(page)).toContain("ADAWORD");
  await reviewAs(page, "Bob");
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
}

test("a second reviewer can type inside another author's suggestion", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await adaSuggests(page);

  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
  await page.keyboard.type("XX");

  // The keystrokes exist. Before, they were dropped with only a status line.
  await expect.poll(() => documentText(page)).toContain("ADAXXWORD");
  await expect(page.locator("#status")).not.toContainText(/isn't supported/i);

  // And they are BOB's suggestion, not silently absorbed into Ada's. Ada's range
  // is now two cards — "ADA" and "WORD" — because typing inside it genuinely
  // split it into two `w:ins` ranges, and a group whose members are no longer
  // adjacent cannot stay one decision (see the reject-all test below). What must
  // hold is that no card claims another author's text.
  const sidebar = page.locator("#reviewSidebarBody");
  await expect(sidebar).toContainText(/Added .ADA./);
  await expect(sidebar).toContainText(/Added .WORD./);
  await expect(sidebar).toContainText(/Added .XX./);
  const cards = await sidebar.evaluate((el) =>
    [...el.querySelectorAll(".review-margin-card")].map((card) => card.innerText.replace(/\s+/g, " ")),
  );
  const bobs = cards.filter((text) => text.includes("Bob"));
  expect(bobs, "Bob's edit is not attributed to Bob").toHaveLength(1);
  expect(bobs[0], "Bob's card is claiming Ada's text").toMatch(/XX/);
  expect(bobs[0]).not.toMatch(/ADA|WORD/);
  for (const card of cards.filter((text) => text.includes("Ada"))) {
    expect(card, "an Ada card is claiming Bob's text").not.toMatch(/XX/);
  }

  expect(consoleErrors).toEqual([]);
});

test("a second reviewer can delete another author's suggestion", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await adaSuggests(page);

  for (let i = 0; i < 7; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Backspace");

  // Accepted, not refused. The deletion nests inside Ada's insertion rather than
  // discarding it, so her authorship survives in the file.
  await expect(page.locator("#status")).not.toContainText(/isn't supported/i);
  await expect.poll(() => documentText(page)).not.toContain("ADAWORD");

  expect(consoleErrors).toEqual([]);
});

// The property a first attempt at this fix broke, which is why it is asserted
// here: `editor_group` marks one authored action, and `validate_review_group`
// requires a group's members to sit at CONSECUTIVE top-level indices — exactly
// what a split breaks, since the point of splitting is to put another author's
// content between the halves. Keeping the group made "Reject all" fail
// validation and silently decide nothing. The halves are therefore ungrouped:
// two `w:ins` elements, which is what they are in OOXML terms anyway.
test("rejecting everything restores the document after authors interleave", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const original = await documentText(page);

  await adaSuggests(page);
  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
  await page.keyboard.type("XX");
  await expect.poll(() => documentText(page)).toContain("ADAXXWORD");

  await page.locator("#reviewRejectAll").click();

  await expect
    .poll(() => documentText(page), { timeout: 15_000 })
    .toBe(original);

  expect(consoleErrors).toEqual([]);
});
