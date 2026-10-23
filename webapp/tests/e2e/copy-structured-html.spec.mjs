// docs/174 TA-12: copying a heading, a list or a table wrote one `<p>` per
// paragraph to `text/html`, so a paste into Word or Google Docs arrived as flat
// paragraphs. The visible HTML now carries the block structure the engine
// reports (`copyBlockOutline`): h1-h6, nested ul/ol, and table.
import {
  MOD,
  applyParagraphStyle,
  clickIntoFirstPage,
  expect,
  gotoEditor,
  moveCaretToDocStart,
  test,
} from "./fixtures.mjs";

async function clipboardEvent(page, type, data = {}) {
  return page.evaluate(
    ({ type, data }) => {
      const dt = new DataTransfer();
      for (const [mime, value] of Object.entries(data)) dt.setData(mime, value);
      document.dispatchEvent(new ClipboardEvent(type, { clipboardData: dt, bubbles: true, cancelable: true }));
      return { html: dt.getData("text/html"), text: dt.getData("text/plain") };
    },
    { type, data },
  );
}

/** What an external app sees: the HTML without our internal round-trip marker. */
const visible = (html) => html.replace(/^<!--[\s\S]*?-->/, "");

test("copied HTML keeps headings, lists and tables for other apps", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // A heading, then an external list and table pasted below it (the
  // structured external-paste path builds real list items and a real table).
  await page.keyboard.type("QZTitle");
  await page.keyboard.press("Enter");
  await page.keyboard.press("ArrowUp");
  await applyParagraphStyle(page, "Heading 1");
  await page.keyboard.press("ArrowDown");
  await clipboardEvent(page, "paste", {
    "text/html": "<ul><li>QZalpha</li><li>QZbeta</li></ul><table><tr><td>QZc1</td><td>QZc2</td></tr><tr><td>QZc3</td><td>QZc4</td></tr></table>",
    "text/plain": "QZalpha\nQZbeta\nQZc1\tQZc2\nQZc3\tQZc4",
  });

  await moveCaretToDocStart(page);
  await page.keyboard.press(`${MOD}+a`);
  const html = visible((await clipboardEvent(page, "copy")).html);

  expect(html).toContain("<h1>QZTitle</h1>");
  expect(html).toMatch(/<ul><li>QZalpha<\/li><li>QZbeta<\/li><\/ul>/);
  expect(html).toMatch(
    /<table><tbody><tr><td><p>QZc1<\/p><\/td><td><p>QZc2<\/p><\/td><\/tr><tr><td><p>QZc3<\/p><\/td><td><p>QZc4<\/p><\/td><\/tr><\/tbody><\/table>/,
  );
  // Nothing flattened: no list item or cell text is a bare paragraph.
  expect(html).not.toContain("<p>QZalpha</p>");
  expect(html).not.toContain("<p>QZTitle</p>");
  expect(consoleErrors).toEqual([]);
});
