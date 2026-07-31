// docs/67 row 9 (accessibility). The document is painted to a canvas, so its
// content is opaque to a screen reader. This slice adds a read-only,
// model-derived off-screen accessibility tree (#a11yDocument) built from the
// engine's accessibilityTree() projection — headings as headings, lists as
// lists, tables as tables — kept in sync with the model. It is never an
// editing surface (the model stays the source of truth).
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

test("the off-screen accessibility tree mirrors the document structure for a screen reader", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  const a11y = page.locator("#a11yDocument");
  await expect(a11y).toHaveAttribute("role", "document");
  await expect(a11y).toHaveAttribute("aria-label", /read-only for assistive technology/i);

  // The corpus title "Rich Document" is a level-1 heading, exposed with the
  // heading role/level (not just styled big text on an opaque canvas).
  const heading = a11y.getByRole("heading", { level: 1 });
  await expect(heading.filter({ hasText: "Rich Document" })).toHaveCount(1);

  // Real structure, not a flat text dump: at least one paragraph and one table.
  expect(await a11y.locator("p").count()).toBeGreaterThan(0);
  expect(await a11y.locator("table").count()).toBeGreaterThan(0);

  // It is read-only — no contenteditable anywhere, and it is out of the tab
  // order (a screen reader browses it; it is not a second editing surface).
  await expect(a11y.locator("[contenteditable]")).toHaveCount(0);
  await expect(a11y).not.toHaveAttribute("contenteditable", /.*/);
  await expect(a11y).not.toHaveAttribute("tabindex", /.*/);

  expect(consoleErrors).toEqual([]);
});

test("a list in the document is exposed as a real list to assistive technology", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // Add a fresh plain body paragraph at the end of the document (the corpus
  // ends in body text, so the new paragraph inherits a plain style — not a
  // heading, which would be classified as a heading rather than a list item).
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
  await page.keyboard.type("BULLETITEMTEXT");

  // Turn it into a bullet list via the ribbon; the off-screen tree must then
  // contain a real <ul><li> carrying the item's text (not a <p>).
  await page.locator("#bulletList").click();
  const a11y = page.locator("#a11yDocument");
  // textContent (not innerText) — the region is visually hidden.
  await expect(a11y.locator("ul > li").filter({ hasText: "BULLETITEMTEXT" })).toHaveCount(1);
  await expect(a11y.locator("p").filter({ hasText: "BULLETITEMTEXT" })).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("the accessibility tree stays in sync when the document is edited", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const marker = "A11YSYNCMARKER";
  await page.keyboard.type(marker);

  // The off-screen mirror rebuilds on the same coalesced frame as the outline,
  // so the typed text appears in it without any explicit refresh.
  await expect(page.locator("#a11yDocument")).toContainText(marker);

  expect(consoleErrors).toEqual([]);
});
