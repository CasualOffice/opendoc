// docs/67 audit row 7 ("clipboard structure"): an internal copy of a table or a
// list must survive paste as real structure, not flatten to plain text. Rich
// run/paragraph/link paste already worked; this covers the STRUCTURED case.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  MOD,
} from "./fixtures.mjs";

// Dispatches a real clipboard event through the editor's own handlers and
// returns the payloads the handler wrote (for copy) so a later paste can replay
// them — the same helper shape the clipboard-rich suite uses.
async function clipboardEvent(page, type, data = {}) {
  return page.evaluate(
    ({ type, data }) => {
      const dt = new DataTransfer();
      for (const [mime, value] of Object.entries(data)) dt.setData(mime, value);
      const event = new ClipboardEvent(type, {
        clipboardData: dt,
        bubbles: true,
        cancelable: true,
      });
      document.dispatchEvent(event);
      return { html: dt.getData("text/html"), text: dt.getData("text/plain") };
    },
    { type, data },
  );
}

test("an internal copy of a table pastes back as a real table, not flattened text", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Insert a 2x2 table; the caret lands in its first cell.
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  // Let the editor surface take focus with the caret settled in the first cell.
  await page.waitForTimeout(200);

  // Type into the first cell and select it — a text range whose endpoints are
  // both inside the table, so the structured copy captures the whole table.
  await page.keyboard.type("CELLTEXT");
  await page.keyboard.press("Shift+Home");
  const clip = await clipboardEvent(page, "copy");
  // The internal marker now carries a structured block fragment (a table), not
  // only flat runs.
  expect(clip.html).toMatch(/^<!--opendoc-clipboard-runs:/);
  const decoded = Buffer.from(
    clip.html.match(/runs:([A-Za-z0-9+/=]+)-->/)[1],
    "base64",
  ).toString("utf8");
  expect(decoded).toContain('"type":"table"');

  // Move the caret to a body paragraph (document start) and paste the table.
  await moveCaretToDocStart(page);
  await expect(page.locator("#tabTable")).toBeDisabled(); // caret is in prose now
  await clipboardEvent(page, "paste", { "text/html": clip.html, "text/plain": clip.text });

  // The paste reconstructed a real table: the caret landed inside it, so the
  // contextual Table ribbon activates. A flattened text paste would leave the
  // caret in a paragraph with the Table tab disabled.
  await expect(page.locator("#tabTable")).toBeEnabled();

  // The cell content survived and now exists twice (original + pasted).
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("CELLTEXT");
  await expect(page.locator("#findStatus")).toHaveText(/ of 2$/);
  await page.keyboard.press("Escape");

  // One undoable action removes the pasted table (Table tab disables again).
  // Use the keyboard: the caret is inside the pasted table, so the ribbon shows
  // its contextual Table tab and the Home tab's Undo button is not visible.
  await page.keyboard.press(`${MOD}+z`);
  await expect(page.locator("#tabTable")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

test("an internal copy of a numbered list pastes back as a list, preserving numbering", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Build three clean paragraphs at the top: two list items and a prose
  // destination below them, none entangled with the fixture's first heading.
  await page.keyboard.press("Enter");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.type("ITEMALPHA");
  await page.keyboard.press("Enter");
  await page.keyboard.type("ITEMBETA");
  await page.keyboard.press("Enter");
  await page.keyboard.type("PROSEDEST");

  // Select the two items (end of BETA up to start of ALPHA) and number them.
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("End");
  await page.keyboard.press("Shift+Home");
  await page.keyboard.press("Shift+ArrowUp");
  await page.keyboard.press("Shift+Home");
  await page.locator("#numberedList").click();
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");
  // Let the list-formatting edit commit and restore the range before copying.
  await page.waitForTimeout(200);

  const clip = await clipboardEvent(page, "copy");
  const decoded = Buffer.from(
    clip.html.match(/runs:([A-Za-z0-9+/=]+)-->/)[1],
    "base64",
  ).toString("utf8");
  expect(decoded).toContain('"numbering"');

  // Collapse to the prose destination (third paragraph) and confirm it is not a
  // list, then paste the copied list there.
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Home");
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "false");
  await clipboardEvent(page, "paste", { "text/html": clip.html, "text/plain": clip.text });

  // The caret landed in the first pasted list item: numbering survived (a
  // flattened paste would drop into plain prose with the button unpressed).
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");

  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("ITEMALPHA");
  await expect(page.locator("#findStatus")).toHaveText(/ of 2$/);
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});
