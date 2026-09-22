// The Home ribbon's Styles group is ONE dropdown — Google Docs' shape — offering a
// short list of styles, each row drawn in the style it applies (docs/115). Picking a
// row applies that named paragraph style over the current selection through
// `setParagraphStyle` via `runToolbarEdit`, so it inherits the Viewing/Suggesting
// gating and history. This spec proves the three behaviours the control must
// guarantee: a row applies a real style (reflected back from `paragraphStyleAt` into
// `data-active-style` and the trigger's label), the apply is a single undoable
// action, and it fails closed in read-only Viewing mode.
//
// What the SHORT list offers, and that nothing became unreachable, is
// `styles-control.spec.mjs`.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  reflectedParagraphStyle as reflectedStyle,
  setReviewMode,
} from "./fixtures.mjs";

/** Opens the Styles menu and leaves it open. */
async function openStyles(page) {
  await page.locator("#stylesTrigger").click();
  await expect(page.locator("#stylesMenu")).toBeVisible();
}

// Picks an offered style that differs from the one currently applied, so choosing it
// is a genuine change with something to undo. Leaves the menu OPEN on that row.
async function cardForADifferentStyle(page) {
  const current = await reflectedStyle(page);
  await openStyles(page);
  const styles = await page.$$eval("#stylesMenu .style-option", (rows) =>
    rows.map((row) => row.dataset.style),
  );
  const target = styles.find((s) => s && s !== current);
  expect(
    target,
    "the menu should offer at least one style other than the caret's",
  ).toBeTruthy();
  return target;
}

test("picking a style applies it and the change undoes as one action", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const before = await reflectedStyle(page);
  const target = await cardForADifferentStyle(page);
  await page
    .locator(`#stylesMenu .style-option[data-style="${target}"]`)
    .click();

  // The visible change: the caret's paragraph now carries the chosen style
  // (`data-active-style` reflects `paragraphStyleAt`) and the trigger names it.
  await expect.poll(() => reflectedStyle(page)).toBe(target);
  await expect(page.locator("#stylesTriggerLabel")).toHaveText(target);
  // Picking a style closes the menu, as every menu-shaped picker does.
  await expect(page.locator("#stylesMenu")).toBeHidden();
  // The apply is a real, undoable edit.
  await expect(page.locator("#undoBtn")).toBeEnabled();

  // A single undo restores the original paragraph style — one user action.
  await page.locator("#undoBtn").click();
  await expect.poll(() => reflectedStyle(page)).toBe(before);
  await expect(page.locator("#stylesTriggerLabel")).toHaveText(before);

  expect(consoleErrors).toEqual([]);
});

test("picking a style is blocked in read-only Viewing mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const before = await reflectedStyle(page);
  const target = await cardForADifferentStyle(page);
  await page.keyboard.press("Escape");

  await setReviewMode(page, "viewing");
  await expect(page.locator("#viewingBanner")).toBeVisible();

  await openStyles(page);
  await page
    .locator(`#stylesMenu .style-option[data-style="${target}"]`)
    .click();

  // The mutation fails closed: the read-only status is emitted, the paragraph
  // style is unchanged, and the trigger still names the style the caret is in.
  await expect(page.locator("#status")).toContainText("read-only");
  await expect.poll(() => reflectedStyle(page)).toBe(before);
  await expect(page.locator("#stylesTriggerLabel")).toHaveText(before);

  expect(consoleErrors).toEqual([]);
});

test("arrow keys move focus down the open menu (listbox keyboard model)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await openStyles(page);
  const rows = page.locator("#stylesMenu .style-option");
  expect(await rows.count()).toBeGreaterThan(1);

  // Focus the first row, then walk down and back; the focused row's data-style is
  // the observable proof focus moved.
  const focusedStyle = () =>
    page.evaluate(
      () =>
        document.activeElement?.closest?.(".style-option")?.dataset.style ??
        null,
    );

  await rows.first().focus();
  const first = await focusedStyle();
  expect(first).toBeTruthy();

  await page.keyboard.press("ArrowDown");
  const second = await focusedStyle();
  expect(second).toBeTruthy();
  expect(second).not.toBe(first);

  await page.keyboard.press("ArrowUp");
  expect(await focusedStyle()).toBe(first);

  expect(consoleErrors).toEqual([]);
});

// A test for the "▾ More styles" popover used to live here. That popover listed every
// paragraph style in the document as cards — the same long list the deleted
// `#paragraphStyle` select carried, reached through a side door — and it is gone
// (docs/115 §5): the band offers a short list, the full stylesheet lives in the
// command palette and Paragraph properties. The reachability it was proving is now
// proved against those surfaces, in `styles-control.spec.mjs`, which is stronger: it
// requires EVERY defined style to have a palette row rather than one example.
