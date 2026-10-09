// A re-render is background work; it must not take the status line off the
// reader.
//
// `renderAll` used to announce "Rendering N pages…" through `setStatus` and then,
// finding its own text still on the line, clear it. Nothing awaits between the
// two writes, so the progress text was never painted — but the line was erased.
// Any render that ran after a command had reported something wiped the report:
// the font-upgrade repaint after an open erased "Footnote added", which is how
// every footnote cell in `surface-editing-matrix.spec.mjs` went red on `main`
// whenever the fonts landed after the insert.
//
// A zoom change re-renders on demand, so it drives the same path without
// depending on when a download finishes.
import { MOD, clickIntoFirstPage, expect, gotoEditor, test } from "./fixtures.mjs";

test("a re-render leaves the reader's status message on the line", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+a`);
  const status = page.locator("#status");
  await expect(status).toHaveText("Document selected");

  const zoom = page.locator("#zoom");
  const before = await zoom.inputValue();
  await page.locator("#zoomIn").click();
  // The render really ran: without this the assertion below would also pass if
  // the click did nothing.
  await expect(zoom).not.toHaveValue(before);
  await expect(status).toHaveText("Document selected");
  expect(consoleErrors).toEqual([]);
});
