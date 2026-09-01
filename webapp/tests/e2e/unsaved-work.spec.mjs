// The document lives in the wasm heap and nowhere else — no server, no
// autosave, no local copy — so anything that discards it without asking is
// data loss, not a rough edge. Three separate holes made that possible and
// each is proved closed here:
//
//   * the dirty flag was written by eight call sites and read by none, and the
//     table/list apply path never wrote it at all, so structural edits left the
//     document looking untouched;
//   * nothing was listening for unload, so Cmd+R or closing the tab discarded
//     every edit silently;
//   * `openBytes` freed the live document BEFORE parsing the new bytes, so a
//     file the engine rejected destroyed the document already on screen and
//     left `doc` pointing at freed wasm memory.
//
// These assertions are all observable through the product surface (the state
// pill, a real cancelable beforeunload, and continued editability) rather than
// through internals, so they keep holding if the implementation moves.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

const stateText = (page) => page.locator("#documentStateText");

/** Dispatches a real cancelable `beforeunload` and reports whether the page
 *  asked the browser to stop. Playwright cannot drive the native dialog, but
 *  `defaultPrevented` is exactly the signal the browser itself acts on. */
async function unloadIsGuarded(page) {
  return page.evaluate(() => {
    const event = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(event);
    return event.defaultPrevented;
  });
}

test("a freshly opened document does not claim to have unsaved work", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  await expect(stateText(page)).toHaveText("Opened");
  // Guarding an untouched document would train the user to dismiss the prompt,
  // which is how a real warning gets ignored later.
  expect(await unloadIsGuarded(page)).toBe(false);

  expect(consoleErrors).toEqual([]);
});

test("typing marks the document edited and arms the unload guard", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.type("x");

  await expect(stateText(page)).toHaveText("Edited");
  expect(await unloadIsGuarded(page)).toBe(true);

  expect(consoleErrors).toEqual([]);
});

// The regression that motivated the shared choke point. Every table and list
// structural edit goes through `runNodeEdit`, which marked nothing — so the
// unload guard would have been silently wrong for exactly the edits a user is
// least able to reproduce from memory.
test("a table/list structural edit marks the document edited", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await expect(stateText(page)).toHaveText("Opened");

  // A list conversion is the cheapest structural edit reachable from the
  // ribbon, and it takes the same apply path as cell shading and table borders.
  await page.locator("#bulletList").click();

  await expect(stateText(page)).toHaveText("Edited");
  expect(await unloadIsGuarded(page)).toBe(true);

  expect(consoleErrors).toEqual([]);
});

test("renaming the document counts as unsaved work", async ({ page, consoleErrors }) => {
  await gotoEditor(page);

  // Rename never touches the model, so no engine revision can observe it — but
  // it changes what Save produces, so it is genuinely unsaved work.
  await page.locator("#docTitle").fill("renamed-by-test.docx");
  await page.locator("#docTitle").press("Enter");

  await expect(stateText(page)).toHaveText("Edited");
  expect(await unloadIsGuarded(page)).toBe(true);

  expect(consoleErrors).toEqual([]);
});

// No `consoleErrors` assertion here: a rejected file is SUPPOSED to log the
// engine's reason. Its absence would be the defect.
test("a file the engine rejects leaves the open document alive and editable", async ({
  page,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.type("survivor");
  await expect(page.locator("#a11yDocument")).toContainText("survivor");

  // A file that announces itself as a ZIP and then is not one: the extension
  // filter admits it, the package reader gets far enough to commit to it, and
  // the engine rejects it. That is the exact shape that used to destroy the
  // live document, because the old code freed it before parsing.
  await page.locator("#file").setInputFiles({
    name: "corrupt.docx",
    mimeType:
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: Buffer.from([0x50, 0x4b, 0x03, 0x04, ...Array(64).fill(0x41)]),
  });

  await expect(page.locator("#status")).toContainText(/could not open/i);
  // The previous document is what the user still has, so the editor must say so
  // rather than implying it is now empty.
  await expect(page.locator("#status")).toContainText(/still here/i);

  // The real proof, and it has to be content: asserting the state pill or a
  // visible page would pass even against the bug, because a destroyed document
  // stays painted and the pill still reads "Edited" from the typing above. Only
  // the document text can tell the difference.
  await expect(page.locator("#a11yDocument")).toContainText("survivor");

  // And it must still ACCEPT edits — before the fix this threw
  // "null pointer passed to rust" and the text never changed.
  // A distinctive marker rather than a positional assertion: the caret lands
  // wherever `clickIntoFirstPage` puts it, and what matters is that the edit
  // reached the model at all.
  await clickIntoFirstPage(page);
  await page.keyboard.type("stillalive");
  await expect(page.locator("#a11yDocument")).toContainText("stillalive");
});
