// Automates the "Native clipboard fidelity" P0 row from
// docs/67-EDITOR-UX-GAP-ANALYSIS.md: copy/cut/paste should preserve rich runs
// (formatting, links) via an internal payload, with sanitized HTML import
// from external apps and plain text as the final fallback. Scoped to
// paragraphs/runs/links (tables/lists remain plain text, per the PR's
// documented non-goals).
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

async function dispatchClipboardEvent(page, type, data = {}) {
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
      return {
        html: dt.getData("text/html"),
        text: dt.getData("text/plain"),
      };
    },
    { type, data },
  );
}

test("copying a bolded selection produces rich HTML with an internal round-trip marker", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Heading 1 is effectively bold through its paragraph style. Move to the
  // first genuinely non-bold line so clicking Bold applies (rather than clears)
  // explicit bold formatting.
  for (let line = 0; line < 20; line++) {
    if ((await page.locator("#bold").getAttribute("aria-pressed")) === "false") break;
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Home");
  }
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "false");

  const marker = "RICHWORD";
  await page.keyboard.type(marker);
  await page.keyboard.press("Shift+Home");
  await page.locator("#bold").click();

  const clip = await dispatchClipboardEvent(page, "copy");
  expect(clip.text).toBe(marker);
  expect(clip.html).toMatch(/^<!--opendoc-clipboard-runs:/);
  expect(clip.html).toMatch(new RegExp(`<b>${marker}</b>`));

  await page.locator("#undoBtn").click(); // bold
  await page.locator("#undoBtn").click(); // typed text
  expect(consoleErrors).toEqual([]);
});

test("pasting external HTML (no internal marker) lands as recognizable text", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await dispatchClipboardEvent(page, "paste", {
    "text/html": "<p><b>Hello</b> <i>world</i></p>",
    "text/plain": "Hello world",
  });

  await page.keyboard.press(`${process.platform === "darwin" ? "Meta" : "Control"}+f`);
  await page.locator("#findInput").fill("Hello world");
  await expect(page.locator("#findStatus")).toHaveText("1 match");
  await page.keyboard.press("Escape");

  await page.locator("#undoBtn").click();
  expect(consoleErrors).toEqual([]);
});

test("pasting the internal marker round-trips without reparsing the visible HTML", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const marker = "ROUNDTRIPWORD";
  await page.keyboard.type(marker);
  await page.keyboard.press("Shift+Home");
  await page.locator("#bold").click();
  const clip = await dispatchClipboardEvent(page, "copy");

  await moveCaretToDocStart(page);
  await dispatchClipboardEvent(page, "paste", {
    "text/html": clip.html,
    "text/plain": clip.text,
  });

  await page.keyboard.press(`${process.platform === "darwin" ? "Meta" : "Control"}+f`);
  await page.locator("#findInput").fill(marker);
  // The typed original and the pasted copy sit adjacently, so the marker
  // genuinely occurs twice now that the status reports a real count instead
  // of a hardcoded "1 match".
  await expect(page.locator("#findStatus")).toHaveText(/ of 2$/);
  await page.keyboard.press("Escape");

  await page.locator("#undoBtn").click(); // pasted copy
  await page.locator("#undoBtn").click(); // bold
  await page.locator("#undoBtn").click(); // typed text
  expect(consoleErrors).toEqual([]);
});
