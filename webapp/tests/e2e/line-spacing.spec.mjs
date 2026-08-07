// Custom line spacing (Word / Google-Docs standard): the spacing popover keeps
// the quick multiples (Single/1.15/1.5/Double) and adds a "Line spacing options"
// block with a mode select (Multiple / At least / Exactly) and a value field.
//   - Multiple rides `doc.setLineSpacing` (the `auto` percent rule).
//   - At least / Exactly ride `doc.setLineSpacingExact(twips, at_least)`.
// These specs prove a custom multiple and an Exactly-pt value take effect (via
// the control's own engine-backed reflection), are one undo each, and are
// blocked in Viewing mode.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
} from "./fixtures.mjs";

const spacingBtn = "#spacingBtn";
const spacingMenu = "#spacingMenu";
const modeSel = "#lineSpacingMode";
const valueInput = "#lineSpacingValue";

// Opens the spacing popover (idempotent: only clicks the trigger when closed)
// and waits for it to be visible so its fields reflect the caret paragraph.
async function openSpacingMenu(page) {
  if (await page.locator(spacingMenu).isHidden()) {
    await page.locator(spacingBtn).click();
  }
  await expect(page.locator(spacingMenu)).toBeVisible();
}

// Closes the popover by clicking a neutral point outside it.
async function closeSpacingMenu(page) {
  if (await page.locator(spacingMenu).isVisible()) {
    await page.keyboard.press("Escape");
    await expect(page.locator(spacingMenu)).toBeHidden();
  }
}

// Commits a custom mode + value through the popover (change fires on blur).
async function applyCustom(page, mode, value) {
  await openSpacingMenu(page);
  await page.locator(modeSel).selectOption(mode);
  await page.locator(valueInput).fill(value);
  await page.locator(valueInput).blur();
}

test("custom Multiple line spacing takes effect and is a single undo", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await openSpacingMenu(page);
  const initialValue = await page.locator(valueInput).inputValue();

  // Apply a multiple the quick presets do not offer (1.75x).
  await applyCustom(page, "multiple", "1.75");

  // The control reflects the engine round-trip: Multiple mode, unit "×", 1.75.
  await expect(page.locator(modeSel)).toHaveValue("multiple");
  await expect(page.locator("#lineSpacingUnit")).toHaveText("×");
  await expect(page.locator(valueInput)).toHaveValue("1.75");

  // Exactly one undo returns the paragraph to its starting spacing.
  await closeSpacingMenu(page);
  await expect(page.locator("#undoBtn")).toBeEnabled();
  await page.locator("#undoBtn").click();
  await openSpacingMenu(page);
  await expect(page.locator(valueInput)).toHaveValue(initialValue);

  expect(consoleErrors).toEqual([]);
});

test("Exactly (pt) line spacing takes effect and is a single undo", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await openSpacingMenu(page);
  const initialMode = await page.locator(modeSel).inputValue();
  const initialValue = await page.locator(valueInput).inputValue();

  // Apply a fixed 24 pt line height (Word "Exactly" → lineRule="exact").
  await applyCustom(page, "exact", "24");

  // Reflected as Exactly, unit "pt", 24 — proving setLineSpacingExact(exact) ran.
  await expect(page.locator(modeSel)).toHaveValue("exact");
  await expect(page.locator("#lineSpacingUnit")).toHaveText("pt");
  await expect(page.locator(valueInput)).toHaveValue("24");

  // One undo reverts both the rule and the value.
  await closeSpacingMenu(page);
  await expect(page.locator("#undoBtn")).toBeEnabled();
  await page.locator("#undoBtn").click();
  await openSpacingMenu(page);
  await expect(page.locator(modeSel)).toHaveValue(initialMode);
  await expect(page.locator(valueInput)).toHaveValue(initialValue);

  expect(consoleErrors).toEqual([]);
});

test("At least (pt) line spacing uses the atLeast rule", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await applyCustom(page, "atLeast", "18");

  await expect(page.locator(modeSel)).toHaveValue("atLeast");
  await expect(page.locator("#lineSpacingUnit")).toHaveText("pt");
  await expect(page.locator(valueInput)).toHaveValue("18");

  expect(consoleErrors).toEqual([]);
});

test("custom line spacing is blocked in Viewing mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Capture the untouched spacing, then go read-only.
  await openSpacingMenu(page);
  const initialValue = await page.locator(valueInput).inputValue();
  await closeSpacingMenu(page);

  await setReviewMode(page, "viewing");
  await expect(page.locator("#viewingBanner")).toBeVisible();

  // Attempting a custom multiple fails closed: read-only banner, no engine change.
  await applyCustom(page, "multiple", "3.0");
  await expect(page.locator("#status")).toContainText("read-only");
  await expect(page.locator(valueInput)).toHaveValue(initialValue);

  expect(consoleErrors).toEqual([]);
});
