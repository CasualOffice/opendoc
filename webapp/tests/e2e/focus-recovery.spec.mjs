// Automates the "Focus ownership and stale gestures" P0 smokes described in
// docs/67-EDITOR-UX-GAP-ANALYSIS.md (row: "Canvas never freezes the caret
// after canceled drags, toolbar clicks, modal/chip interactions, or page-gap
// movement"). These were previously verified by an agent manually dispatching
// synthetic events during a PR and narrating the result (see P1G-FOCUS-001 /
// P1G-SELECTION-ROBUST-001 in docs/14-EXECUTION-TRACKER.md); this suite makes
// that check permanent and automatic.
import { test, expect, gotoEditor, clickIntoFirstPage, typeMoveFindUndo } from "./fixtures.mjs";

// Every recovery scenario proves the editor is still usable the same way:
// after the interrupt, type a distinctive marker, find it, then undo it.
// That end-to-end path only succeeds if the caret/focus state healed.
async function assertRecovered(page, marker) {
  await typeMoveFindUndo(page, marker);
}

// Presses the mouse down on the first page, then drags near the bottom edge
// of the viewport so `startSelectionAutoScroll`'s rAF loop (main.js ~708-736)
// starts nudging `#viewport.scrollTop` every frame — leaving the editor
// mid-drag (`dragging === true`) with an active auto-scroll loop for the
// caller to interrupt. Deliberately never releases the mouse: a genuine
// pointercancel/blur/hidden-tab has no guaranteed following pointerup, which
// is exactly the case `resetPointerGesture` exists to recover from.
async function startDragNearBottomEdge(page) {
  const pageBox = await page.locator(".page-wrap .page").first().boundingBox();
  const viewportBox = await page.locator("#viewport").boundingBox();
  await page.mouse.move(pageBox.x + 30, pageBox.y + 30);
  await page.mouse.down();
  await page.mouse.move(pageBox.x + 30, viewportBox.y + viewportBox.height - 10);
}

function viewportScrollTop(page) {
  return page.locator("#viewport").evaluate((el) => el.scrollTop);
}

// Confirms the interrupt actually stopped the auto-scroll rAF loop, not just
// that some time passed — without a working `resetPointerGesture`, `dragging`
// stays true and the loop keeps nudging `scrollTop` on every frame forever.
async function assertAutoScrollStopped(page) {
  await page.waitForTimeout(50);
  const atInterrupt = await viewportScrollTop(page);
  await page.waitForTimeout(300);
  expect(await viewportScrollTop(page)).toBe(atInterrupt);
}

test.describe("focus ownership and stale gestures", () => {
  test("pointercancel mid-drag stops auto-scroll and does not freeze the caret", async ({
    page,
    consoleErrors,
  }) => {
    await gotoEditor(page);
    await startDragNearBottomEdge(page);
    await page.waitForTimeout(200);
    expect(await viewportScrollTop(page)).toBeGreaterThan(0); // drag is actually auto-scrolling

    await page.evaluate(() => window.dispatchEvent(new PointerEvent("pointercancel")));
    await assertAutoScrollStopped(page);
    await page.mouse.up();

    // A fresh, ordinary click proves the editor is fully usable again, rather
    // than typing into whatever huge range the interrupted drag last selected.
    await clickIntoFirstPage(page);
    await assertRecovered(page, "OPDOC-POINTERCANCEL-1");
    expect(consoleErrors).toEqual([]);
  });

  test("window blur mid-drag stops auto-scroll and does not freeze the caret", async ({
    page,
    consoleErrors,
  }) => {
    await gotoEditor(page);
    await startDragNearBottomEdge(page);
    await page.waitForTimeout(200);
    expect(await viewportScrollTop(page)).toBeGreaterThan(0);

    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await assertAutoScrollStopped(page);
    await page.mouse.up();

    await clickIntoFirstPage(page);
    await assertRecovered(page, "OPDOC-BLUR-2");
    expect(consoleErrors).toEqual([]);
  });

  test("hidden tab mid-drag stops auto-scroll and does not freeze the caret", async ({
    page,
    consoleErrors,
  }) => {
    await gotoEditor(page);
    await startDragNearBottomEdge(page);
    await page.waitForTimeout(200);
    expect(await viewportScrollTop(page)).toBeGreaterThan(0);

    await page.evaluate(() => {
      Object.defineProperty(document, "hidden", { value: true, configurable: true });
      document.dispatchEvent(new Event("visibilitychange"));
    });
    await assertAutoScrollStopped(page);
    await page.mouse.up();

    await clickIntoFirstPage(page);
    await assertRecovered(page, "OPDOC-HIDDENTAB-3");
    expect(consoleErrors).toEqual([]);
  });

  test("clicking a toolbar button returns focus to the page surface", async ({
    page,
    consoleErrors,
  }) => {
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    await expect(page.locator("#pages")).toBeFocused();

    await page.locator("#bold").click();
    await expect(page.locator("#pages")).toBeFocused();

    await assertRecovered(page, "OPDOC-TOOLBAR-4");
    expect(consoleErrors).toEqual([]);
  });

  test("clicking the canvas and typing works (baseline)", async ({ page, consoleErrors }) => {
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    await expect(page.locator("#pages")).toBeFocused();

    await assertRecovered(page, "OPDOC-BASELINE-5");
    expect(consoleErrors).toEqual([]);
  });
});
