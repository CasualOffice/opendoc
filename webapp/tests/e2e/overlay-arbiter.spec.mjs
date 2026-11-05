// The floating-chrome arbiter (docs/184 D-12; `floating_arbiter.mjs`).
//
// Audit CANVAS-06 measured a single click on a link opening the link card
// (404x88) AND the selection toolbar (254x40), the card covering more than half
// of the toolbar; Tab into a table cell raising the toolbar over the row above;
// and CRITIC-10 the paste chip drawn over the second pasted paragraph. The
// guarantee, in every state: at most ONE floating surface shows, and it does
// not cover the selection.
import { clickIntoFirstPage, expect, gotoEditor, moveCaretToDocStart, stableBox, test } from "./fixtures.mjs";

const SURFACES = ["#linkChip", ".object-context-bar", ".table-insert-target", "#selToolbar", "#pasteOptions"];

/** Every floating surface on screen, and the painted selection's rectangle. */
async function floating(page) {
  return page.evaluate((selectors) => {
    const shown = [];
    for (const selector of selectors) {
      for (const el of document.querySelectorAll(selector)) {
        const r = el.getBoundingClientRect();
        const style = getComputedStyle(el);
        if (r.width > 0 && r.height > 0 && style.display !== "none" && style.visibility !== "hidden" && Number(style.opacity) > 0) {
          shown.push({ selector, left: r.left, top: r.top, right: r.right, bottom: r.bottom });
        }
      }
    }
    let selection = null;
    const marks = document.querySelectorAll("#pages .overlay .highlight, #pages .overlay .table-cell-selection");
    const parts = marks.length ? marks : document.querySelectorAll("#pages .overlay .caret");
    for (const el of parts) {
      const r = el.getBoundingClientRect();
      if (!r.width && !r.height) continue;
      selection = selection
        ? { left: Math.min(selection.left, r.left), top: Math.min(selection.top, r.top), right: Math.max(selection.right, r.right), bottom: Math.max(selection.bottom, r.bottom) }
        : { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    }
    return { shown, selection };
  }, SURFACES);
}

const overlaps = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;

/** The guarantee, asserted on whatever state the page is in. */
async function expectOneClearSurface(page, what) {
  const { shown, selection } = await floating(page);
  expect(shown.length, `${what}: ${shown.map((s) => s.selector).join(", ")}`).toBeLessThanOrEqual(1);
  if (shown.length && selection) {
    expect(overlaps(shown[0], selection), `${what}: ${shown[0].selector} covers the selection`).toBe(false);
  }
  return shown.map((s) => s.selector);
}

async function makeLink(page, marker) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(`${marker} `);
  await moveCaretToDocStart(page);
  for (let i = 0; i < marker.length; i += 1) await page.keyboard.press("Shift+ArrowRight");
  const box = await stableBox(page.locator(".overlay .highlight").first());
  await page.keyboard.press(`${process.platform === "darwin" ? "Meta" : "Control"}+k`);
  await expect(page.locator("#linkDialog")).toBeVisible();
  await page.locator("#linkUrlInput").fill("https://example.com/arbiter");
  await page.locator("#linkUrlInput").press("Enter");
  await expect(page.locator("#linkDialog")).toBeHidden();
  return box;
}

test("a click on a link places the caret and shows only the link card", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  const link = await makeLink(page, "ARBITERLINK");
  await page.keyboard.press("ArrowRight"); // collapse the selection the dialog left
  await page.mouse.click(link.x + link.width * 0.4, link.y + link.height / 2);
  await expect(page.locator("#linkChip")).toBeVisible();
  // A caret, not a selection of the whole link (CANVAS-06/16).
  await expect(page.locator("#pages .overlay .highlight")).toHaveCount(0);
  await expect(page.locator("#pages .overlay .caret")).toHaveCount(1);
  await expect(page.locator("#selToolbar")).toBeHidden();
  expect(await expectOneClearSurface(page, "link click")).toEqual(["#linkChip"]);
  expect(consoleErrors).toEqual([]);
});

test("the mini-toolbar is earned by a pointer selection, not by a keyboard one", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("POINTERMADE KEYBOARDMADE ");
  await moveCaretToDocStart(page);
  // Keyboard: Shift+arrows select, and no toolbar rises over the line above.
  for (let i = 0; i < 11; i += 1) await page.keyboard.press("Shift+ArrowRight");
  await expect(page.locator("#pages .overlay .highlight").first()).toBeVisible();
  await expect(page.locator("#selToolbar")).toBeHidden();
  await expectOneClearSurface(page, "keyboard selection");

  // Pointer: a drag across the same word raises it, clear of the selection.
  const word = await stableBox(page.locator("#pages .overlay .highlight").first());
  await page.keyboard.press("ArrowLeft");
  await page.mouse.move(word.x + 2, word.y + word.height / 2);
  await page.mouse.down();
  await page.mouse.move(word.x + word.width / 2, word.y + word.height / 2, { steps: 3 });
  await page.mouse.move(word.x + word.width - 2, word.y + word.height / 2, { steps: 3 });
  await page.mouse.up();
  await expect(page.locator("#selToolbar")).toBeVisible();
  expect(await expectOneClearSurface(page, "pointer selection")).toEqual(["#selToolbar"]);

  // The next key retires it, as Word's does.
  await page.keyboard.press("Shift+ArrowRight");
  await expect(page.locator("#selToolbar")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("Tab into a table cell raises no toolbar over the row above", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.keyboard.type("A1");
  await page.keyboard.press("Tab");
  await page.keyboard.type("B1");
  await page.keyboard.press("Shift+Tab"); // Tab back selects the cell's text
  await expect(page.locator("#selToolbar")).toBeHidden();
  await expectOneClearSurface(page, "tab into cell");
  expect(consoleErrors).toEqual([]);
});

test("the paste chip sits at the end of the paste, collapsed, off the pasted text", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Enter");
  await page.keyboard.press("ArrowUp");
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.setData("text/html", "<p>First pasted paragraph</p><p>Para with <b>bold</b> and more</p>");
    dt.setData("text/plain", "First pasted paragraph\nPara with bold and more");
    document.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }));
  });
  const chip = page.locator("#pasteOptionsToggle");
  await expect(chip).toBeVisible();
  // Collapsed: the choices are behind the chip, as Word's (Ctrl) chip.
  await expect(page.locator("#pasteOptionsMenu")).toBeHidden();
  const chipBox = await stableBox(chip);
  expect(chipBox.width, "a chip, not a row of buttons").toBeLessThanOrEqual(48);
  // At the end of the paste: beside the caret the paste left there.
  const caret = await stableBox(page.locator("#pages .overlay .caret"));
  expect(chipBox.x).toBeGreaterThanOrEqual(caret.x + caret.width);
  expect(chipBox.y + chipBox.height).toBeGreaterThan(caret.y);
  expect(chipBox.y).toBeLessThan(caret.y + caret.height);
  await expectOneClearSurface(page, "after paste");

  // A click opens the four choices; Keep source is the one in effect.
  await chip.click();
  await expect(page.locator("#pasteOptionsKeepSource")).toHaveAttribute("aria-checked", "true");
  for (const id of ["#pasteOptionsKeepSource", "#pasteOptionsMerge", "#pasteOptionsTextOnly", "#pasteOptionsPicture"]) {
    await expect(page.locator(id)).toBeVisible();
  }
  await page.keyboard.press("Escape");
  await expect(page.locator("#pasteOptions")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("when two surfaces want the screen, the higher one wins and the other returns after", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  const link = await makeLink(page, "PRIORITYLINK");
  await page.keyboard.press("ArrowRight"); // a press inside a selection would drag it
  // A pointer selection raises the mini-toolbar…
  await page.mouse.move(link.x + 2, link.y + link.height / 2);
  await page.mouse.down();
  await page.mouse.move(link.x + link.width - 2, link.y + link.height / 2, { steps: 4 });
  await page.mouse.up();
  await expect(page.locator("#selToolbar")).toBeVisible();
  // …and the paste chip is offered while it is up (its owner holds it open).
  // This is the arbiter's own choke point: the owners never coordinate.
  await page.evaluate(() => {
    document.getElementById("pasteOptions").hidden = false;
  });
  await expect(page.locator("#pasteOptions")).toBeHidden();
  expect(await expectOneClearSurface(page, "toolbar over paste chip")).toEqual(["#selToolbar"]);
  // The toolbar goes; the chip its owner still holds comes back.
  await page.evaluate(() => {
    document.getElementById("selToolbar").hidden = true;
  });
  await expect(page.locator("#pasteOptions")).toBeVisible();
  await page.evaluate(() => {
    document.getElementById("pasteOptions").hidden = true;
  });
  expect(consoleErrors).toEqual([]);
});
