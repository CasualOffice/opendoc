// Covers the find/replace panel redesign (docs/67 row "Find/replace depth"):
// the prev/next/close buttons previously referenced a CSS class with zero
// rules anywhere in the stylesheet (rendered as unstyled native buttons), the
// status always read a hardcoded "1 match" regardless of the real count, and
// there was no "Replace all" or toolbar entry point. This locks the fixed
// behavior: real match counting, Replace all, and the new ribbon button.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

test("the ribbon Find button opens the panel", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await expect(page.locator("#findPanel")).toBeHidden();
  await page.locator("#findBtn").click();
  await expect(page.locator("#findPanel")).toBeVisible();
  await expect(page.locator("#findInput")).toBeFocused();

  await page.keyboard.press("Escape");
  await expect(page.locator("#findPanel")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("find reports an accurate count across repeated words, not a hardcoded one", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // A single, unique match still reads as the plain singular form (existing
  // fixtures elsewhere depend on this exact string).
  await page.keyboard.press(`${MOD}+f`);
  const findInput = page.locator("#findInput");
  await findInput.fill("Nested A");
  await expect(page.locator("#findStatus")).toHaveText("1 match");
  await page.keyboard.press("Escape");

  // Seed deterministic repeated text (not relying on incidental word counts
  // in the demo fixture's prose) and confirm it reports "x of 3", not a
  // lying "1 match".
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.setData("text/plain", "REPEATREPEATREPEAT");
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
  });
  await page.keyboard.press(`${MOD}+f`);
  await findInput.fill("REPEAT");
  await expect(page.locator("#findStatus")).toHaveText(/^\d+ of 3$/);

  await findInput.fill("zzz_no_such_text_zzz");
  await expect(page.locator("#findStatus")).toHaveText("No match");
  await page.keyboard.press("Escape");

  await page.locator("#undoBtn").click(); // the seeding paste
  expect(consoleErrors).toEqual([]);
});

test("replace all replaces every occurrence and is fully undoable", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Seed deterministic repeated text via the rich-paste path (same technique
  // fixtures.mjs / cross-structure-delete.spec.mjs already use).
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.setData("text/plain", "MARKMARKMARK");
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
  });

  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("MARK");
  await expect(page.locator("#findStatus")).toHaveText(/^\d+ of 3$/);

  await page.locator("#replaceInput").fill("done");
  await page.locator("#replaceAll").click();
  await expect(page.locator("#findStatus")).toHaveText("Replaced 3");

  await page.locator("#findInput").fill("MARK");
  await expect(page.locator("#findStatus")).toHaveText("No match");
  await page.locator("#findInput").fill("done");
  await expect(page.locator("#findStatus")).toHaveText(/^\d+ of 3$/);
  await page.keyboard.press("Escape");

  // Undo repeatedly (bounded) until "MARKMARKMARK" is back — this doesn't
  // assume a specific undo-stack granularity for replaceAllMatches, only
  // that the whole operation is fully reversible. Move the caret to the
  // document start before each check: undo re-parks the caret wherever the
  // just-restored text sits, and a forward find that starts *inside* the
  // very match it's looking for can miss it (a pre-existing findText wrap
  // limitation, not something this fix touches — see docs/14 tracker note).
  const undoBtn = page.locator("#undoBtn");
  let restored = false;
  for (let i = 0; i < 10 && !(await undoBtn.isDisabled()); i++) {
    await undoBtn.click();
    await clickIntoFirstPage(page);
    await moveCaretToDocStart(page);
    await page.keyboard.press(`${MOD}+f`);
    await page.locator("#findInput").fill("MARKMARKMARK");
    const status = await page.locator("#findStatus").textContent();
    await page.keyboard.press("Escape");
    if (status === "1 match") {
      restored = true;
      break;
    }
  }
  expect(restored).toBe(true);

  // Best-effort cleanup of the seeding paste — each test gets a fresh page
  // via gotoEditor, so this isn't load-bearing; bounded so a stuck button
  // state can't hang the run.
  for (let i = 0; i < 5 && !(await undoBtn.isDisabled()); i++) await undoBtn.click();

  expect(consoleErrors).toEqual([]);
});
