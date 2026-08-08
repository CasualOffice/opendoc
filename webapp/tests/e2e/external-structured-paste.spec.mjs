// EXTERNAL structured paste: a `<table>` or `<ul>`/`<ol>` copied from another app
// (Excel/Word/Google Docs/a web page) carries no OpenDoc round-trip marker, so it
// used to flatten to plain runs. It must now paste as REAL structure — a table, or
// bullet/numbered list paragraphs — as one undoable action. (The internal
// OpenDoc->OpenDoc structured path is covered by structured-paste.spec.mjs.)
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  MOD,
} from "./fixtures.mjs";

// Dispatches a real paste event carrying foreign `text/html` through the editor's
// own handlers — the same DataTransfer synthesis the other paste suites use. No
// internal marker, so the clipboard bridge parses the raw DOM.
async function pasteHtml(page, html, text = "") {
  return page.evaluate(
    ({ html, text }) => {
      const dt = new DataTransfer();
      dt.setData("text/html", html);
      if (text) dt.setData("text/plain", text);
      const event = new ClipboardEvent("paste", {
        clipboardData: dt,
        bubbles: true,
        cancelable: true,
      });
      document.dispatchEvent(event);
    },
    { html, text },
  );
}

test("an external HTML table pastes as a real table with all cells; one undo reverts", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // The caret is in prose, so the contextual Table ribbon is disabled up front.
  await expect(page.locator("#tabTable")).toBeDisabled();

  // A plain 2x2 external table (the shape Excel/Docs put on the clipboard).
  const html = `<table><tbody>
    <tr><td>CELLAA</td><td>CELLAB</td></tr>
    <tr><td>CELLBA</td><td>CELLBB</td></tr>
  </tbody></table>`;
  await pasteHtml(page, html, "CELLAA\tCELLAB\nCELLBA\tCELLBB");

  // A real table was reconstructed: the caret landed inside it, so the contextual
  // Table ribbon activates (a flattened text paste would leave the caret in prose
  // with the Table tab disabled).
  await expect(page.locator("#tabTable")).toBeEnabled();

  // All four cell texts survived.
  for (const needle of ["CELLAA", "CELLAB", "CELLBA", "CELLBB"]) {
    await page.keyboard.press(`${MOD}+f`);
    await page.locator("#findInput").fill(needle);
    await expect(page.locator("#findStatus")).toHaveText("1 match");
    await page.keyboard.press("Escape");
  }

  // One undoable action removes the whole pasted table (Table tab disables again).
  await page.keyboard.press(`${MOD}+z`);
  await expect(page.locator("#tabTable")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

test("an external HTML bullet list pastes as list-item paragraphs", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "false");

  // A plain external bullet list.
  const html = `<ul><li>ITEMONE</li><li>ITEMTWO</li></ul>`;
  await pasteHtml(page, html, "ITEMONE\nITEMTWO");

  // The caret landed in a real bullet item: the Bulleted-list button lights up (a
  // flattened paste would drop into plain prose with the button unpressed).
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");

  // Both items exist as distinct paragraphs.
  for (const needle of ["ITEMONE", "ITEMTWO"]) {
    await page.keyboard.press(`${MOD}+f`);
    await page.locator("#findInput").fill(needle);
    await expect(page.locator("#findStatus")).toHaveText("1 match");
    await page.keyboard.press("Escape");
  }

  // One undoable action reverts the whole list paste.
  await page.keyboard.press(`${MOD}+z`);
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "false");

  expect(consoleErrors).toEqual([]);
});
