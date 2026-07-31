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
  const box = await marker.boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.locator(".overlay .checklist-marker").first()).toHaveAttribute(
    "aria-checked",
    "true",
  );

  // And back to unchecked.
  const box2 = await page.locator(".overlay .checklist-marker").first().boundingBox();
  await page.mouse.click(box2.x + box2.width / 2, box2.y + box2.height / 2);
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
  const box = await marker.boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.locator("#status")).toContainText("read-only");
  await expect(page.locator(".overlay .checklist-marker").first()).toHaveAttribute(
    "aria-checked",
    "false",
  );

  expect(consoleErrors).toEqual([]);
});
