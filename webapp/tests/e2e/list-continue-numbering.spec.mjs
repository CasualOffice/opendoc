// Coverage for docs/67 list-lifecycle row 5's "continue" affordance
// (P1G-LIST-*): Restart numbering splits a numbered list into a second
// sequence; Continue numbering is then offered on that split item and rejoins
// it to the earlier list. The Continue control is available ONLY when there is
// an earlier numbered list at the same level to resume (`canContinueList`), so
// its enabled/disabled state is the DOM-observable proof of the behavior.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  MOD,
} from "./fixtures.mjs";

test("Restart then Continue numbering splits and rejoins a numbered list", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Three paragraphs, then make all three one numbered list.
  await page.keyboard.type("Alpha");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Beta");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Gamma");
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.press(`Shift+${MOD}+End`);
  await page.locator("#numberedList").click();
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");

  const continueBtn = page.locator("#continueList");
  const restartBtn = page.locator("#restartList");

  // Caret on the second item. It is contiguous with the first, so there is
  // nothing to continue yet — Continue is disabled, Restart is available.
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.press("ArrowDown");
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");
  await expect(continueBtn).toBeDisabled();
  await expect(restartBtn).toBeEnabled();

  // Restart at the second item: it (and the contiguous third) become a new
  // sequence. Continue is now offered because an earlier list exists to resume.
  await restartBtn.click();
  await expect(continueBtn).toBeEnabled();

  // On the FIRST item there is no earlier numbered list, so Continue stays off.
  await page.keyboard.press(`${MOD}+Home`);
  await expect(continueBtn).toBeDisabled();

  // Back on the restarted second item, Continue rejoins it to the first list;
  // once contiguous again there is nothing left to continue, so it disables.
  await page.keyboard.press("ArrowDown");
  await expect(continueBtn).toBeEnabled();
  await continueBtn.click();
  await expect(continueBtn).toBeDisabled();
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");

  // The rejoin is one undoable action: undo re-splits the list, re-enabling
  // Continue on the second item.
  await page.locator("#undoBtn").click();
  await expect(continueBtn).toBeEnabled();

  expect(consoleErrors).toEqual([]);
});

test("Continue numbering is reachable from the command palette", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.keyboard.type("One");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Two");
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.press(`Shift+${MOD}+End`);
  await page.locator("#numberedList").click();

  // Split the second item off with Restart, then resume via the palette.
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.press("ArrowDown");
  await page.locator("#restartList").click();
  await expect(page.locator("#continueList")).toBeEnabled();

  await page.locator("#searchTrigger").click();
  const palette = page.locator("#cmdInput");
  await expect(palette).toBeFocused();
  await palette.fill("Continue numbering");
  await page.locator(".cmd-item", { hasText: "Continue numbering" }).first().click();
  await expect(page.locator("#continueList")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});
