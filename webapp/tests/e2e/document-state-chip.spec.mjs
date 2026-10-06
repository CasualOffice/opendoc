// The document-state chip reports the state of a DOCUMENT, so it has nothing to
// say until one exists.
//
// It said "Opened" from the first frame: the chip's resting markup is
// `data-state="opened"` with the text "Opened", and the rule that hides the
// document chrome until `.doc-loaded` listed the findings chip beside it and not
// the chip itself. So the boot frame, and the bare `?blank=1` upload screen for
// as long as it stays up, told a reader a document had been opened over an empty
// drop card (`desk-01a`, `phone-01a`). Word and Google Docs show no saved-state
// indicator before a document exists.
import { test, expect } from "./fixtures.mjs";
import { makeLargeDocx } from "./large-docx.mjs";

test("the state chip is absent until a document is open, then reads Opened", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?blank=1");
  // The engine is up and the editor is waiting for a file — the state a reader
  // actually sees, for as long as they take to choose one.
  await expect(page.locator("#status")).toContainText("Ready");
  await expect(page.locator("#drop")).toBeVisible();
  await expect(page.locator("#documentState"), "a state chip with no document to describe").toBeHidden();

  await page.setInputFiles("#file", {
    name: "state-chip.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: Buffer.from(makeLargeDocx(1)),
  });
  await expect(page.locator("body")).toHaveClass(/doc-loaded/);
  // The control half: the same rule must not hide it once there IS a document,
  // or the guard above would pass on a chip that never appears at all.
  await expect(page.locator("#documentState")).toBeVisible();
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "opened");
  expect(consoleErrors).toEqual([]);
});
