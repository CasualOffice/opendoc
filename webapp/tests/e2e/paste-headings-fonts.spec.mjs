// docs/174 TA-11: HTML pasted from another app dropped its headings (an <h1>
// became body text) and its font family (Georgia became the default face), and
// said nothing. A heading now takes Heading 1-6 (created when the document has
// no such style), the face is kept, and whatever the paste could NOT keep is
// said in the status line.
import { MOD, clickIntoFirstPage, expect, gotoEditor, moveCaretToDocStart, reflectedParagraphStyle, test } from "./fixtures.mjs";

async function pasteHtml(page, html, text) {
  await page.evaluate(
    ({ html, text }) => {
      const data = new DataTransfer();
      data.setData("text/html", html);
      data.setData("text/plain", text);
      document.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
    },
    { html, text },
  );
}

/** Puts the caret on `text` with Find, so what the toolbar reflects is THAT paragraph's. */
async function caretOn(page, text) {
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(text);
  await expect(page.locator("#findStatus")).not.toHaveText("No match");
  await page.keyboard.press("Escape");
}

test("a pasted h1 and h3 become Heading 1 and Heading 3, and the font face is kept", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // At the END of the document, in a body paragraph, so a heading cannot be
  // inherited from the paragraph the paste lands in.
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
  await expect.poll(() => reflectedParagraphStyle(page)).not.toMatch(/^Heading/);
  await pasteHtml(
    page,
    '<h1>QZ Title</h1><h3>QZ Section</h3><p><span style="font-family:\'Georgia\',serif">QZ body in Georgia</span></p>',
    "QZ Title\nQZ Section\nQZ body in Georgia",
  );
  await expect.poll(() => page.locator("#a11yDocument").innerText()).toContain("QZ body in Georgia");

  await caretOn(page, "QZ Title");
  await expect.poll(() => reflectedParagraphStyle(page)).toBe("Heading 1");
  await caretOn(page, "QZ Section");
  await expect.poll(() => reflectedParagraphStyle(page)).toBe("Heading 3");
  await caretOn(page, "body in Georgia");
  await expect(page.locator("#fontFamilyLabel")).toHaveText("Georgia");
  // A clean structured paste reports nothing.
  await expect(page.locator("#status")).not.toContainText("ordinary paragraphs");
  expect(consoleErrors).toEqual([]);
});

test("where the structure cannot be rebuilt, the paste says what became plain paragraphs, and pictures", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  // A range selection: the engine only rebuilds structure at a collapsed body
  // caret, so this goes through the flat fallback.
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Shift+ArrowRight");
  await pasteHtml(page, "<h2>QZ Flat</h2><ul><li>QZ item</li></ul><img src=\"x.png\" alt=\"\">", "QZ Flat\nQZ item");
  await expect.poll(() => page.locator("#a11yDocument").innerText()).toContain("QZ Flat");
  await expect(page.locator("#status")).toContainText("Pasted, but headings and lists became ordinary paragraphs here.");
  await expect(page.locator("#status")).toContainText("Pictures in the pasted content were not brought across.");
  expect(consoleErrors).toEqual([]);
});
