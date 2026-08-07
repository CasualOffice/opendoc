// The floating selection toolbar (`#selToolbar`, shown above a text selection)
// used to carry a raw OS `<input type=color>` for text color and a bare 5-entry
// `<select>` for highlight (which also shipped a duplicate `value="none"` bug).
// Those are replaced by the SAME swatch-picker popovers the ribbon uses, opened
// from `#selTextColorBtn` / `#selHighlightBtn` into `#selTextColorMenu` /
// `#selHighlightMenu`, applying through the ribbon's `applyTextColor` /
// `applyHighlight` path (one undoable action, gated in Viewing/Suggesting).
//
// The proof that "the run got it" is not the button swatch (the apply reflects
// that directly) but the reflected ribbon bar after the selection is re-queried:
// `updateToolbar` reads the run's actual style back out of the engine.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
} from "./fixtures.mjs";

// Selects `count` characters forward from the current caret.
async function selectForward(page, count) {
  for (let i = 0; i < count; i += 1) await page.keyboard.press("Shift+ArrowRight");
}

// Collapses to the document start and reselects `count` chars so the toolbar
// reflects the run's persisted style (not just the just-clicked swatch).
async function reselectFromStart(page, count) {
  await moveCaretToDocStart(page);
  await selectForward(page, count);
}

function barColor(page, id) {
  return page.locator(id).evaluate((el) => getComputedStyle(el).backgroundColor);
}

test("floating toolbar text-color picker opens the ribbon swatch menu and applies a color (one undo)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 6);

  // Selecting text reveals the floating bar; its text-color control is now a
  // button (the old raw `<input type=color>` is gone).
  await expect(page.locator("#selToolbar")).toBeVisible();
  await expect(page.locator("#selColor")).toHaveCount(0);

  // Baseline: the first 12 characters start with a uniform text color (the
  // control is not in its mixed state), so a later mixed reading is meaningful.
  const textCtl = page.locator(".color-control:not(.color-control-highlight)");
  await reselectFromStart(page, 12);
  await expect(textCtl).not.toHaveClass(/is-mixed/);

  await reselectFromStart(page, 6);
  await page.locator("#selTextColorBtn").click();
  const menu = page.locator("#selTextColorMenu");
  await expect(menu).toBeVisible();
  await expect(menu).toContainText("Standard colors");
  await expect(menu.locator(".color-row-action[data-auto]")).toContainText("Automatic");
  await menu.locator('[data-color="#ff0000"]').click();
  await expect(menu).toBeHidden();

  // The run persisted the color: reselect and the ribbon bar reflects it from
  // the engine's run style.
  await reselectFromStart(page, 6);
  await expect.poll(() => barColor(page, "#textColorBar")).toBe("rgb(255, 0, 0)");

  // Only the first 6 chars are red, so extending the selection to 12 reports a
  // mixed text color — a signal read straight from the engine's run style.
  await reselectFromStart(page, 12);
  await expect(textCtl).toHaveClass(/is-mixed/);

  // Exactly one undoable action: a single undo removes the color, so the same
  // 12-char span is uniform again.
  await page.locator("#undoBtn").click();
  await reselectFromStart(page, 12);
  await expect(textCtl).not.toHaveClass(/is-mixed/);

  expect(consoleErrors).toEqual([]);
});

test("floating toolbar highlight picker applies a named highlight and clears it (No color)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 6);

  await expect(page.locator("#selToolbar")).toBeVisible();
  // The old bare <select> (and its duplicate value="none") is gone.
  await expect(page.locator("#selHighlight")).toHaveCount(0);

  await page.locator("#selHighlightBtn").click();
  const menu = page.locator("#selHighlightMenu");
  await expect(menu).toBeVisible();
  // Exactly one "No color" reset entry (the duplicate-none bug is fixed).
  await expect(menu.locator('[data-highlight="none"]')).toHaveCount(1);
  await expect(menu.locator('[data-highlight="none"]')).toContainText("No color");
  await menu.locator('[data-highlight="green"]').click();
  await expect(menu).toBeHidden();

  await reselectFromStart(page, 6);
  await expect.poll(() => barColor(page, "#highlightBar")).toBe("rgb(0, 255, 0)");

  // Clearing the highlight (No color) removes it from the run.
  await page.locator("#selHighlightBtn").click();
  await menu.locator('[data-highlight="none"]').click();
  await expect(menu).toBeHidden();
  await reselectFromStart(page, 6);
  await expect.poll(() => barColor(page, "#highlightBar")).toBe("rgba(0, 0, 0, 0)");

  expect(consoleErrors).toEqual([]);
});

test("floating toolbar color pickers are blocked in Viewing mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "viewing");

  // Selection still works in Viewing, so the floating bar appears.
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 6);
  await expect(page.locator("#selToolbar")).toBeVisible();

  // Choosing a text-color swatch reports read-only instead of applying.
  await page.locator("#selTextColorBtn").click();
  await page.locator('#selTextColorMenu [data-color="#ff0000"]').click();
  await expect(page.locator("#status")).toContainText("read-only");

  // The run was not changed: the first 12 chars stay uniform (no red boundary
  // was introduced), read straight from the engine's run style.
  const textCtl = page.locator(".color-control:not(.color-control-highlight)");
  await reselectFromStart(page, 12);
  await expect(textCtl).not.toHaveClass(/is-mixed/);

  // The highlight picker is likewise blocked and leaves the run un-highlighted.
  await reselectFromStart(page, 6);
  await page.locator("#selHighlightBtn").click();
  await page.locator('#selHighlightMenu [data-highlight="green"]').click();
  await expect(page.locator("#status")).toContainText("read-only");
  await reselectFromStart(page, 6);
  await expect.poll(() => barColor(page, "#highlightBar")).toBe("rgba(0, 0, 0, 0)");

  expect(consoleErrors).toEqual([]);
});
