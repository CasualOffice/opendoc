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

// A picture in the document must be ANNOUNCED. The engine gained an `image`
// node (#511) carrying the author's alt text; before the host rendered it, the
// node fell through to the generic branch and became an empty `<p>` — a figure
// announced as silence, which is worse than announcing it badly.
test("a picture is announced, with the author's alt text when there is one", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  const images = page.locator("#a11yDocument img");
  await expect.poll(() => images.count()).toBeGreaterThan(0);
  const count = await images.count();
  expect(count).toBeGreaterThan(0);

  // Every announced figure carries a non-empty accessible name. An empty alt
  // would hide the graphic from a reader entirely, and the engine cannot know
  // the author meant it to be decorative.
  for (let i = 0; i < count; i += 1) {
    const alt = await images.nth(i).getAttribute("alt");
    expect(alt, `figure ${i} has no accessible name`).toBeTruthy();
    expect(alt.trim().length).toBeGreaterThan(0);
  }

  expect(consoleErrors).toEqual([]);
});

// A table's header geometry is what lets a reader hear "Revenue, Q3" while
// moving through cells, instead of a bare grid of numbers. The engine reports
// which rows are headers (#511); the host has to render them as `th` with a
// scope, or the information is thrown away on the last step.
test("a header row reaches the accessibility tree as th scope=col", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Insert a table and mark its first row as a repeating header — the same
  // control a user would use.
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.locator("#tabTable").click();

  const before = await page.locator('#a11yDocument thead th[scope="col"]').count();
  expect(before, "the fresh table has no header row yet").toBe(0);

  await page.locator("#tablePropertiesBtn").click();
  await page.locator("#tableHeaderRow").check();

  // The projection now reports row 0 as a header, and the tree must say so.
  await expect
    .poll(() => page.locator('#a11yDocument thead th[scope="col"]').count())
    .toBeGreaterThan(0);
});
