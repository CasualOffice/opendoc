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

  // Open now asks before replacing unsaved work (HF-002), and this document has
  // some. Answer it, because the failure being proved here is what happens
  // AFTER the user has agreed to the replacement — the engine rejecting the
  // bytes must not take the live document with it.
  await expect(page.locator("#confirmDialog")).toBeVisible();
  await page.locator("#confirmAccept").click();

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

// The unload guard closes the "close the tab" half of HF-002. This is the other
// half: Open replaced a dirty document with no prompt at all, on both the file
// picker and the drag-and-drop path. Both are routed through one confirm, so
// both are asserted — a guard on one path and not the other is how this class
// of defect keeps coming back.
test("opening another file over unsaved work asks first, and keeping means keeping", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.type("KEEPME");
  await expect(page.locator("#a11yDocument")).toContainText("KEEPME");

  await page.locator("#file").setInputFiles("sample.docx");
  const prompt = page.locator("#confirmDialog");
  await expect(prompt).toBeVisible();
  await expect(prompt).toContainText(/unsaved/i);

  await page.locator("#confirmCancel").click();
  await expect(prompt).toBeHidden();
  // Refused means refused: the same document, the same edit, still dirty.
  await expect(page.locator("#a11yDocument")).toContainText("KEEPME");
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
  expect(await unloadIsGuarded(page)).toBe(true);
  expect(consoleErrors).toEqual([]);
});

test("opening another file over a clean document does not ask", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  // Nothing has been typed, so there is nothing to lose and nothing to ask
  // about — a prompt here would be the "needless warning" half of the tradeoff.
  await page.locator("#file").setInputFiles("sample.docx");
  await expect(page.locator("#confirmDialog")).toBeHidden();
  await expect(page.locator("#docTitle")).toHaveValue("sample.docx");
  expect(consoleErrors).toEqual([]);
});

test("a dropped file over unsaved work asks the same question", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.type("DROPME");
  await expect(page.locator("#a11yDocument")).toContainText("DROPME");

  // Synthesize the drop the viewport listens for. The picker and the drop both
  // land in handleFile, which is the point: one question, asked once.
  await page.evaluate(() => {
    const transfer = new DataTransfer();
    transfer.items.add(new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], "dropped.docx"));
    document
      .getElementById("viewport")
      .dispatchEvent(new DragEvent("drop", { dataTransfer: transfer, bubbles: true, cancelable: true }));
  });

  await expect(page.locator("#confirmDialog")).toBeVisible();
  await page.locator("#confirmCancel").click();
  await expect(page.locator("#a11yDocument")).toContainText("DROPME");
});
