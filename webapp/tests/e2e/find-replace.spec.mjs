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
  await expect(page.locator(".find-panel-title")).toHaveText("Find and replace");
  await expect(page.locator(".find-options")).toBeVisible();
  await expect(page.locator(".find-actions")).toBeVisible();

  const desktopLayout = await page.evaluate(() => {
    const panel = document.querySelector("#findPanel").getBoundingClientRect();
    const rows = [
      ".find-panel-head",
      ".find-search-row",
      ".find-options",
      ".find-replacement",
      ".find-actions",
    ].map((selector) => document.querySelector(selector).getBoundingClientRect());
    return {
      contained: rows.every(
        (row) =>
          row.left >= panel.left - 1 &&
          row.right <= panel.right + 1 &&
          row.top >= panel.top - 1 &&
          row.bottom <= panel.bottom + 1,
      ),
      ordered: rows.every((row, index) => index === 0 || row.top >= rows[index - 1].bottom - 1),
    };
  });
  expect(desktopLayout).toEqual({ contained: true, ordered: true });

  await page.setViewportSize({ width: 480, height: 720 });
  const narrowLayout = await page.evaluate(() => {
    const panel = document.querySelector("#findPanel");
    const rect = panel.getBoundingClientRect();
    return {
      left: rect.left,
      right: rect.right,
      viewportWidth: document.documentElement.clientWidth,
      panelOverflow: panel.scrollWidth - panel.clientWidth,
      documentOverflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    };
  });
  expect(narrowLayout.left).toBeGreaterThanOrEqual(0);
  expect(narrowLayout.right).toBeLessThanOrEqual(narrowLayout.viewportWidth);
  expect(narrowLayout.panelOverflow).toBeLessThanOrEqual(1);
  expect(narrowLayout.documentOverflow).toBeLessThanOrEqual(0);

  await page.keyboard.press("Escape");
  await expect(page.locator("#findPanel")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("previous and next scroll the canvas to the selected match", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const marker = "FIND_SCROLL_TARGET";
  await moveCaretToDocStart(page);
  await page.keyboard.type(marker);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.type(marker);
  await moveCaretToDocStart(page);

  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(marker);
  const initialStatus = await page.locator("#findStatus").textContent();
  expect(initialStatus).toMatch(/^[12] of 2$/);

  const firstScroll = await page.locator("#viewport").evaluate((el) => el.scrollTop);
  const panelTop = (await page.locator("#findPanel").boundingBox()).y;
  await page.locator("#findNext").click();
  await expect(page.locator("#findStatus")).not.toHaveText(initialStatus);
  const nextState = await page.evaluate(() => {
    const viewport = document.querySelector("#viewport");
    const viewportRect = viewport.getBoundingClientRect();
    const highlight = document.querySelector(".overlay .highlight").getBoundingClientRect();
    return {
      scrollTop: viewport.scrollTop,
      visible: highlight.top >= viewportRect.top && highlight.bottom <= viewportRect.bottom,
      panelTop: document.querySelector("#findPanel").getBoundingClientRect().top,
    };
  });
  expect(Math.abs(nextState.scrollTop - firstScroll)).toBeGreaterThan(10);
  expect(nextState.visible).toBe(true);
  expect(nextState.panelTop).toBeCloseTo(panelTop, 0);

  await page.locator("#findPrev").click();
  await expect(page.locator("#findStatus")).toHaveText(initialStatus);
  const previousScroll = await page.locator("#viewport").evaluate((el) => el.scrollTop);
  expect(Math.abs(previousScroll - nextState.scrollTop)).toBeGreaterThan(10);

  await page.keyboard.press("Escape");
  await page.locator("#undoBtn").click();
  await page.locator("#undoBtn").click();
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
