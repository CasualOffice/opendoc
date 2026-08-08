// Enter at the end of a heading starts body text, not another heading.
//
// Word and Docs both do this, and Word's mechanism is the style's `w:next`. The
// model already stored it (`Style::next`) — imported from every .docx the editor
// opens — but the Enter path ignored it, so typing a heading and pressing Enter
// left the caret in Heading 1 and the next sentence became a heading too. It is
// the kind of defect a user hits within a minute of starting a document.
//
// The document's own heading is the fixture: the assertion is about what the
// paragraph Enter STARTS is, so it is read from the model-derived accessibility
// tree (h1/h2/p), which is canvas-independent.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

const blocks = (page) =>
  page.evaluate(() =>
    [...document.querySelectorAll("#a11yDocument > *")]
      .slice(0, 4)
      .map((el) => `${el.tagName}:${(el.textContent || "").replace(/\s+/g, " ").slice(0, 24)}`),
  );

test("Enter at the end of a heading starts a body paragraph", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // Deterministic: the caret must be IN the opening heading for this to test
  // anything. Clicking alone does not guarantee where it lands.
  await moveCaretToDocStart(page);

  // The fixture opens on a level-1 heading.
  expect((await blocks(page))[0]).toMatch(/^H1:/);

  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("body text");

  // Polled: the typed text arrives asynchronously, and reading the projection
  // straight after `type` intermittently caught it a keystroke short.
  await expect.poll(async () => (await blocks(page))[1]).toBe("P:body text");
  expect((await blocks(page))[0]).toMatch(/^H1:/); // the heading itself is untouched

  expect(consoleErrors).toEqual([]);
});

test("splitting a heading in the middle keeps both halves a heading", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Mid-paragraph Enter is one paragraph becoming two, not a new one starting,
  // so the style carries — the same distinction Word draws.
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");

  // Polled: the projection is rebuilt after the edit applies, so reading it
  // immediately after the keystroke intermittently saw the pre-split document.
  await expect.poll(async () => (await blocks(page))[1]).toMatch(/^H1:/);
  expect((await blocks(page))[0]).toMatch(/^H1:/);

  expect(consoleErrors).toEqual([]);
});

test("the style change undoes with the paragraph break", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const before = await blocks(page);

  await moveCaretToDocStart(page);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  expect((await blocks(page))[1]).toMatch(/^P:/);

  // The break and the style it started are one action: undo restores the
  // document exactly, rather than leaving an empty body paragraph behind.
  await page.locator("#undoBtn").click();
  await expect.poll(() => blocks(page)).toEqual(before);

  expect(consoleErrors).toEqual([]);
});
