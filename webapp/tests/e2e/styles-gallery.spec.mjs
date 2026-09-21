// The Home ribbon's Styles group is a Word-style visual gallery of style cards, each
// drawn in its own style — and, since docs/114, the ONE control the band offers for
// paragraph styles. Clicking a card applies that named paragraph style over the
// current selection through `setParagraphStyle` via `runToolbarEdit`, so it inherits
// the Viewing/Suggesting gating and history. This spec proves the three behaviours the
// gallery must guarantee: a card applies a real style (reflected back from
// `paragraphStyleAt` into `data-active-style` and the active card), the apply is a
// single undoable action, and it fails closed in read-only Viewing mode.
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

// Picks a gallery card whose style differs from the one currently applied, so
// clicking it is a genuine change with something to undo. Returns its style name.
async function cardForADifferentStyle(page) {
  const current = await reflectedStyle(page);
  const styles = await page.$$eval("#stylesGallery .style-card", (cards) =>
    cards.map((c) => c.dataset.style),
  );
  const target = styles.find((s) => s && s !== current);
  expect(target, "gallery should offer at least one style other than the caret's").toBeTruthy();
  return target;
}

test("clicking a gallery card applies its style and the change undoes as one action", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const before = await reflectedStyle(page);
  const target = await cardForADifferentStyle(page);
  const card = page.locator(`#stylesGallery .style-card[data-style="${target}"]`);

  await card.click();

  // The visible change: the caret's paragraph now carries the clicked style
  // (`data-active-style` reflects `paragraphStyleAt`) and the card is marked active.
  await expect.poll(() => reflectedStyle(page)).toBe(target);
  await expect(card).toHaveAttribute("aria-selected", "true");
  // The apply is a real, undoable edit.
  await expect(page.locator("#undoBtn")).toBeEnabled();

  // A single undo restores the original paragraph style — one user action.
  await page.locator("#undoBtn").click();
  await expect.poll(() => reflectedStyle(page)).toBe(before);
  await expect(card).toHaveAttribute("aria-selected", String(before === target));

  expect(consoleErrors).toEqual([]);
});

test("a gallery card is blocked in read-only Viewing mode", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const before = await reflectedStyle(page);
  const target = await cardForADifferentStyle(page);
  const card = page.locator(`#stylesGallery .style-card[data-style="${target}"]`);

  await setReviewMode(page, "viewing");
  await expect(page.locator("#viewingBanner")).toBeVisible();

  await card.click();

  // The mutation fails closed: the read-only status is emitted, the paragraph
  // style is unchanged, and the clicked card never becomes active.
  await expect(page.locator("#status")).toContainText("read-only");
  await expect.poll(() => reflectedStyle(page)).toBe(before);
  await expect(card).toHaveAttribute("aria-selected", String(before === target));

  expect(consoleErrors).toEqual([]);
});

test("arrow keys rove focus across the gallery cards (listbox keyboard model)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const cards = page.locator("#stylesGallery .style-card");
  const count = await cards.count();
  expect(count).toBeGreaterThan(1);

  // Focus the current Tab stop, then walk right and back with the arrow keys;
  // the focused card's data-style is the observable proof focus moved.
  const focusedStyle = () =>
    page.evaluate(() => document.activeElement?.closest?.(".style-card")?.dataset.style ?? null);

  await cards.first().focus();
  const first = await focusedStyle();
  expect(first).toBeTruthy();

  await page.keyboard.press("ArrowRight");
  const second = await focusedStyle();
  expect(second).toBeTruthy();
  expect(second).not.toBe(first);

  await page.keyboard.press("ArrowLeft");
  expect(await focusedStyle()).toBe(first);

  expect(consoleErrors).toEqual([]);
});

// A test for the "▾ More styles" popover used to live here. That popover listed every
// paragraph style in the document as cards — the same long list the deleted
// `#paragraphStyle` select carried, reached through a side door — and it is gone
// (docs/114 §5): the band offers a short list, the full stylesheet lives in the
// command palette and Paragraph properties. The reachability it was proving is now
// proved against those surfaces, in `styles-control.spec.mjs`, which is stronger: it
// requires EVERY defined style to have a palette row rather than one example.
