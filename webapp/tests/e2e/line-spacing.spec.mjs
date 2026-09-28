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
  runPaletteCommand,
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

  // Attempting a line-spacing change fails closed: read-only banner, no engine
  // change. Driven from the PALETTE, because Viewing mode composes the Home band
  // away and the popover's trigger with it — and `applyLinePercent` is the very
  // function the custom Multiple field commits through, so this is the same gate
  // on the same path. The engine-backed field is read back once the band returns,
  // which also proves leaving Viewing restores the chrome.
  await runPaletteCommand(page, "paragraph.spacing.200", "Line spacing");
  await expect(page.locator("#status")).toContainText("read-only");
  await setReviewMode(page, "editing");
  await openSpacingMenu(page);
  await expect(page.locator(valueInput)).toHaveValue(initialValue);

  expect(consoleErrors).toEqual([]);
});

// ---------------------------------------------------------------------------
// STYLE-RESOLVED spacing. `paragraphSpacing` now resolves through the style
// cascade and carries `lineFromStyle` / `beforeFromStyle` / `afterFromStyle`.
//
// The defect this guards: the popover read DIRECT formatting only, so on a
// paragraph that takes its line spacing from its style — most paragraphs in most
// documents — nothing was ticked, the box was empty, and a note said in words
// that the value came from somewhere else. Word shows the resolved number; Docs
// ticks the resolved preset; a blank box is neither.
//
// The condition is CREATED here rather than found in the fixture, and the
// precondition is explicit: 1.5 is set on one paragraph, pushed into its style,
// and read back on a DIFFERENT paragraph carrying no direct spacing at all. A
// guard that relied on the fixture happening to define a styled line spacing
// would go green on the day the fixture changed, while proving nothing.

test("the spacing menu ticks 1.5 on a paragraph whose 1.5 comes only from its style", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Create the condition: 1.5 on this paragraph, then written into its style.
  await openSpacingMenu(page);
  await page.locator('.spacing-line[data-percent="150"]').click();
  await runPaletteCommand(page, "style.updateFromSelection", "update style");
  await expect(page.locator("#status")).toContainText("match the selection");

  // A different paragraph, given that style and nothing else. Its 1.5 is
  // inherited: there is no direct `w:spacing` on it at all.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await runPaletteCommand(page, "style.Heading 1", "Style: Heading 1");
  await openSpacingMenu(page);

  // Docs: the resolved preset is ticked.
  const ticked = page.locator('.spacing-line[data-percent="150"]');
  await expect(ticked).toHaveAttribute("aria-checked", "true");
  // Word: the resolved number is in the box.
  await expect(page.locator(valueInput)).toHaveValue("1.5");
  await expect(page.locator(modeSel)).toHaveValue("multiple");
  // …and the distinction Word's dialog drops is kept: the ticked row and the
  // box both say the value was inherited, and the note says so in words.
  await expect(ticked).toHaveAttribute("data-from-style", "true");
  await expect(page.locator(valueInput)).toHaveAttribute("data-from-style", "true");
  await expect(page.locator("#lineSpacingFromStyle")).toBeVisible();
  await expect(page.locator("#lineSpacingFromStyle")).toContainText("from its style");

  // Setting it HERE is a different reading of the same number: same tick, same
  // value, and the inherited marks gone.
  await page.locator('.spacing-line[data-percent="150"]').click();
  await openSpacingMenu(page);
  await expect(ticked).toHaveAttribute("aria-checked", "true");
  await expect(page.locator(valueInput)).toHaveValue("1.5");
  await expect(ticked).not.toHaveAttribute("data-from-style", "true");
  await expect(page.locator("#lineSpacingFromStyle")).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

test("zero space before reads as zero, not as unset", async ({ page, consoleErrors }) => {
  // `ParagraphSpacing::default()` answers -1 for before/after because 0 is a
  // real value. The box therefore prints "0" for a paragraph that says "no space
  // before me", and is empty only when nothing in the cascade says anything —
  // the two states the old `0` default conflated.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await openSpacingMenu(page);
  await page.locator("#spaceBefore").fill("0");
  await page.locator("#spaceBefore").blur();
  await closeSpacingMenu(page);
  await openSpacingMenu(page);
  await expect(page.locator("#spaceBefore")).toHaveValue("0");
  expect(consoleErrors).toEqual([]);
});
