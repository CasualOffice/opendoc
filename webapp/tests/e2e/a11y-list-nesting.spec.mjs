// Nested lists must reach assistive technology as nested lists.
//
// Screen readers announce list depth from the nesting of the lists they are
// given — there is no other channel for it. The engine tracks depth correctly
// (`NumberingRef::level`) and Tab really does demote into it, but the
// accessibility projection carried only `{ ordered, text }`, so every item was
// emitted as a sibling. A three-level list was announced as one flat list, and
// the demotion the user just performed was silent to them.
//
// This spec builds the nesting through the real gesture — Tab, the word-processor
// convention — and reads back the structure, because that is what a screen
// reader consumes.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

async function startListInFreshParagraph(page, control = "#bulletList") {
  await clickIntoFirstPage(page);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.locator(control).click();
}

const listHtml = (page) =>
  page.evaluate(() => document.querySelector("#a11yDocument ul, #a11yDocument ol")?.outerHTML ?? "");

test("Tab-demoted items are exposed as nested lists, not siblings", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await startListInFreshParagraph(page);

  await page.keyboard.type("top");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Tab");
  await page.keyboard.type("child");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Tab");
  await page.keyboard.type("grandchild");

  // Each level is a list inside the item above it — the only structure that
  // conveys depth to a screen reader.
  await expect
    .poll(() => listHtml(page))
    .toBe("<ul><li>top<ul><li>child<ul><li>grandchild</li></ul></li></ul></li></ul>");

  expect(consoleErrors).toEqual([]);
});

test("promoting with Shift+Tab closes the nested list again", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await startListInFreshParagraph(page);

  await page.keyboard.type("one");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Tab");
  await page.keyboard.type("nested");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.type("back");

  // "back" returns to the outer list rather than staying in the inner one.
  await expect
    .poll(() => listHtml(page))
    .toBe("<ul><li>one<ul><li>nested</li></ul></li><li>back</li></ul>");

  expect(consoleErrors).toEqual([]);
});

test("a flat list is still exposed flat", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await startListInFreshParagraph(page, "#numberedList");

  await page.keyboard.type("first");
  await page.keyboard.press("Enter");
  await page.keyboard.type("second");

  // The nesting logic must not invent depth where there is none, and an ordered
  // list is still an <ol>.
  await expect.poll(() => listHtml(page)).toBe("<ol><li>first</li><li>second</li></ol>");
  expect(await page.locator("#a11yDocument ol ol").count()).toBe(0);

  expect(consoleErrors).toEqual([]);
});
