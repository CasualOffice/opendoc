// Automates the "Cross-structure delete/selection" P0 gap
// (docs/67-EDITOR-UX-GAP-ANALYSIS.md): deleting a selection that crosses a
// table boundary used to throw `EditError::Unsupported` from
// `casual-doc-edit`'s `join_paragraphs` (it requires the two paragraphs
// being joined to be siblings in the *same* block list, which a body
// paragraph and a table-cell paragraph never are) and the JS host swallowed
// that as a bare `console.warn`, so it looked like nothing happened. Fixed
// by making `selection_delete_ops` container-boundary-aware (never joins
// across a table/content-control boundary, still clears covered text) and by
// having `runEdit` surface a status-bar message on any rejected edit.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

const MOD = process.platform === "darwin" ? "Meta" : "Control";

async function find(page, query) {
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(query);
  const status = await page.locator("#findStatus").textContent();
  await page.keyboard.press("Escape");
  return status;
}

test("deleting a selection that crosses into the document's table clears text without breaking the table", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Select the real fixture's nested table cell, then extend the selection
  // back to the document start — a deterministic body-into-table-cell range
  // without depending on fragile pixel/line-count assumptions.
  expect(await find(page, "Nested A")).toBe("1 match");
  await page.keyboard.press(`Shift+${MOD}+Home`);

  const warnings = [];
  page.on("console", (msg) => {
    if (msg.type() === "warning") warnings.push(msg.text());
  });

  await page.keyboard.press("Backspace");
  await page.waitForTimeout(200);

  expect(warnings.filter((w) => w.includes("edit ignored"))).toEqual([]);
  expect(consoleErrors).toEqual([]);
  expect(await page.locator("#status").textContent()).not.toMatch(/not supported|isn't supported/i);

  // The table survived: a cell past the deleted range is still there.
  expect(await find(page, "Nested B")).toBe("1 match");

  await page.locator("#undoBtn").click();
});

test("an edit that's still unsupported (a partial hyperlink cut) shows a status error instead of silence", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Build "AB" (hyperlinked) + "CD" (plain) via the rich-paste path, then
  // select [1,4) — starting *inside* the hyperlink, ending outside it.
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.setData("text/html", '<p><a href="https://example.com">AB</a>CD</p>');
    dt.setData("text/plain", "ABCD");
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
  });
  await page.keyboard.press("Home");
  await page.keyboard.press("ArrowRight"); // caret between 'A' and 'B' — inside the link
  await page.keyboard.press("Shift+End"); // selects "BCD": starts inside the link, ends outside it

  await page.keyboard.press("Backspace");
  await page.waitForTimeout(200);

  await expect(page.locator("#status")).toHaveText(/isn't supported/i);

  // Nothing was silently corrupted — the text is untouched.
  const status = await find(page, "ABCD");
  expect(status).toBe("1 match");
  expect(consoleErrors).toEqual([]);

  await page.locator("#undoBtn").click(); // the paste that built the fixture
});
