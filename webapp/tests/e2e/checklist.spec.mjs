import { test, expect, gotoEditor, clickIntoFirstPage, setReviewMode } from "./fixtures.mjs";

// docs/67 row 5 — checklist authoring. A checklist is a bullet list whose marker
// is a checkbox glyph; per-item checked state is which of two numbering
// definitions the item uses. The checkbox marker is an engine-drawn, clickable
// target (model-as-truth) that toggles the item, gated like the other list edits.

test("create a checklist, click its checkbox marker to toggle checked", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Create the checklist on the caret paragraph; the toolbar reflects it.
  await page.locator("#checkList").click();
  await expect(page.locator("#checkList")).toHaveAttribute("aria-pressed", "true");

  // Its checkbox marker is painted as a clickable overlay, starting unchecked.
  const marker = page.locator(".overlay .checklist-marker").first();
  await expect(marker).toBeVisible();
  await expect(marker).toHaveAttribute("aria-checked", "false");

  // Clicking the marker toggles the item to checked (model flips, re-renders).
  await marker.click();
  await expect(page.locator(".overlay .checklist-marker").first()).toHaveAttribute(
    "aria-checked",
    "true",
  );

  // And back to unchecked.
  await page.locator(".overlay .checklist-marker").first().click();
  await expect(page.locator(".overlay .checklist-marker").first()).toHaveAttribute(
    "aria-checked",
    "false",
  );

  // Toggling the checklist off clears the markers.
  await page.locator("#checkList").click();
  await expect(page.locator("#checkList")).toHaveAttribute("aria-pressed", "false");
  await expect(page.locator(".overlay .checklist-marker")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("toggling a checklist item is blocked in Viewing mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator("#checkList").click();
  const marker = page.locator(".overlay .checklist-marker").first();
  await expect(marker).toHaveAttribute("aria-checked", "false");

  // Viewing mode is read-only: the checkbox does not toggle and the read-only
  // status is shown (consistent with the other list edits).
  await setReviewMode(page, "viewing");
  // Use the live locator so an atomic async font render may replace the page
  // between actionability checks without leaving a stale element handle.
  await marker.click();
  await expect(page.locator("#status")).toContainText("read-only");
  await expect(page.locator(".overlay .checklist-marker").first()).toHaveAttribute(
    "aria-checked",
    "false",
  );

  expect(consoleErrors).toEqual([]);
});

// HF-126 — Enter at the end of a TICKED item produced another ticked item.
// A paragraph split clones the paragraph's properties, and a checklist item's
// checked state IS its numbering instance, so the new line arrived already
// completed and struck through before it had any content to complete.
test("Enter after a checked item starts an unchecked one, and a mid-item split does not", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press("Home");

  await page.locator("#checkList").click();
  await expect(page.locator("#checkList")).toHaveAttribute("aria-pressed", "true");

  const markers = page.locator(".overlay .checklist-marker");
  await markers.first().click();
  await expect(markers.first()).toHaveAttribute("aria-checked", "true");

  // End of the ticked item, then Enter: the follow-on item is a NEW task.
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await expect(markers).toHaveCount(2);
  await expect(markers.nth(0)).toHaveAttribute("aria-checked", "true");
  await expect(markers.nth(1)).toHaveAttribute("aria-checked", "false");

  // Typing into the new item must not tick it either.
  await page.keyboard.type("second task");
  await expect(markers.nth(1)).toHaveAttribute("aria-checked", "false");

  // Splitting a ticked item in the MIDDLE is one task becoming two, and both
  // halves keep the state the user set — the opposite rule, and the reason this
  // cannot simply clear `numbering` on every split.
  await markers.nth(1).click();
  await expect(markers.nth(1)).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("End");
  for (let i = 0; i < 5; i++) await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Enter");
  await expect(markers).toHaveCount(3);
  await expect(markers.nth(1)).toHaveAttribute("aria-checked", "true");
  await expect(markers.nth(2)).toHaveAttribute("aria-checked", "true");

  expect(consoleErrors).toEqual([]);
});
