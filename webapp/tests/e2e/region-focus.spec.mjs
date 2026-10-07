// F6 / Shift+F6 move the keyboard between the window's regions — the tab
// strip, any open pane, the document and the status bar — as they do in Word
// and in every Windows application.
//
// Before this there was no keyboard route from the document OUT to the chrome:
// Tab inside the editing surface inserts a tab character, so a keyboard user in
// the document could reach the ribbon, an open pane or the status bar only with
// the mouse. The guard walks the whole cycle in both directions and then types,
// because the claim is not "focus moved" but "the reader can get out AND back,
// and typing still lands in the document" (the focus-owner contract,
// `expectEditorFocused`).
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  expectEditorFocused,
  openFilePage,
  shortcutHint,
} from "./fixtures.mjs";

/** Which region holds the keyboard, by what the reader would call it. */
const regionOf = (page) =>
  page.evaluate(() => {
    const active = document.activeElement;
    if (!active || active === document.body) return "none";
    if (active.id === "pages" || active.id === "editorTextInput") return "document";
    if (active.closest(".footer")) return "status";
    if (active.closest("header.bar, .ribbon, #compactToolbar")) return "navigation";
    const pane = active.closest("aside, #reviewSidebar, #findPanel");
    return pane ? `pane:${pane.id}` : `elsewhere:${active.id || active.tagName}`;
  });

/** Presses `key` until the document has the keyboard again; the regions on the way. */
async function walk(page, key) {
  const visited = [];
  for (let i = 0; i < 10; i += 1) {
    await page.keyboard.press(key);
    visited.push(await regionOf(page));
    if (visited.at(-1) === "document") return visited;
  }
  throw new Error(`${key} never brought the keyboard back to the document: ${visited.join(" → ")}`);
}

test("F6 goes round every region and back to the document; Shift+F6 goes round the other way", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  // A pane on the LEFT of the document, so the cycle has one on each side of it
  // whenever the comments pane is open on the right.
  await page.locator("#railOutline").click();
  await expect(page.locator("#outlinePanel")).toBeVisible();
  await clickIntoFirstPage(page);
  await expectEditorFocused(page);

  const forward = await walk(page, "F6");
  expect(forward).toEqual(expect.arrayContaining(["status", "navigation", "pane:outlinePanel"]));
  // Reading order: the status bar wraps round to the tab strip, which comes
  // before the left pane, which comes before the document.
  expect(forward.indexOf("status")).toBeLessThan(forward.indexOf("navigation"));
  expect(forward.indexOf("navigation")).toBeLessThan(forward.indexOf("pane:outlinePanel"));
  expect(new Set(forward).size, `a region was visited twice: ${forward.join(" → ")}`).toBe(forward.length);

  const backward = await walk(page, "Shift+F6");
  expect(backward.slice(0, -1)).toEqual(forward.slice(0, -1).reverse());

  // Back in the document, typing goes into the document.
  await expectEditorFocused(page);
  await page.keyboard.type("REGIONBACK");
  await expect(page.locator("#a11yDocument")).toContainText("REGIONBACK");
  expect(consoleErrors).toEqual([]);
});

test("F6 lands on the selected ribbon tab, where the arrow keys work at once", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // Shift+F6 from the document is the region just before it: with no pane open
  // on the left, that is the navigation axis.
  await page.keyboard.press("Shift+F6");
  const tab = page.locator('.ribbon-tabs [role="tab"][aria-selected="true"]');
  await expect(tab).toBeFocused();
  const landedOn = await tab.getAttribute("id");
  await page.keyboard.press("ArrowRight");
  await expect(page.locator('.ribbon-tabs [role="tab"]:focus')).toHaveCount(1);
  await expect(page.locator('.ribbon-tabs [role="tab"]:focus')).not.toHaveId(landedOn);
  expect(consoleErrors).toEqual([]);
});

test("File ▸ Shortcuts lists F6 / Shift+F6 as one row", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await openFilePage(page);
  await page.locator('#filePageBody [data-file-pane="shortcuts"]').click();
  const row = page.locator("#shortcutsDialog .shortcuts-row").filter({ hasText: "Move between regions" });
  await expect(row.locator(".shortcuts-keys")).toHaveText(`${shortcutHint("F6")} / ${shortcutHint("⇧F6")}`);
  expect(consoleErrors).toEqual([]);
});

test("F6 leaves the keyboard exactly where it was inside an open dialog", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await page.locator("#propertiesBtn").click();
  await expect(page.locator("#propertiesPanel")).toBeVisible();
  // NOT the dialog's first field. The modal's own trap pulls escaped focus back
  // to its FIRST control, so "focus is still inside" would pass even if F6 had
  // walked out and been dragged back — losing the reader's place in the form.
  // Staying on the field they were in is the property.
  await page.locator("#propCreator").focus();
  await page.keyboard.press("F6");
  await expect(page.locator("#propCreator"), "F6 walked out of a modal").toBeFocused();
  await page.keyboard.press("Shift+F6");
  await expect(page.locator("#propCreator")).toBeFocused();
  expect(consoleErrors).toEqual([]);
});
