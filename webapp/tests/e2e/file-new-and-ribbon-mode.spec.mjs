// Two IA gaps adopted from the owner's prototype.
//
// 1. File ▸ New blank document (docs/105 UX-011, OO-002). There was no
//    `file.new` anywhere in the product: the editor could only edit a file that
//    already existed, so the most basic thing a word processor does — start a
//    document — was the one thing it could not do. The engine exposes no
//    "create document" call, so the host builds a real minimal DOCX package and
//    hands it to the same `open()` path every other document takes.
//
// 2. Ribbon density (docs/104 HF-094). The compact/full choice was already real
//    and already persisted, but the only way to reach it was an unlabelled 28px
//    chevron at the end of the tab strip. A persisted preference with one
//    undiscoverable control is a setting most users will never find.
import {
  test,
  expect,
  definedParagraphStyles,
  gotoEditor,
  clickIntoFirstPage,
  expectEditorFocused,
  MOD,
  saveDocument,
  openAppMenu,
} from "./fixtures.mjs";

async function runFromMenu(page, menu, commandId) {
  await openAppMenu(page, menu);
  const item = page.locator(`#appMenuPopover .app-menu-item[data-command="${commandId}"]`);
  await expect(item).toBeVisible();
  await item.click();
}

async function runFromPalette(page, commandId) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  const row = page.locator(`#cmdList .cmd-item[data-command-id="${commandId}"]`);
  await expect(row).toBeVisible();
  await row.click();
  await expect(page.locator("#cmdPalette")).toBeHidden();
}

test("File ▸ New blank document creates an empty, editable, one-page document", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await runFromMenu(page, "file", "file.new");

  // A blank document, not the sample: one page, and the title bar names it.
  await expect(page.locator("#docTitle")).toHaveValue("Untitled document.docx");
  await expect.poll(() => page.locator(".page-wrap").count()).toBe(1);
  await expect(page.locator("#documentStateText")).toHaveText("Opened");

  // It is where the user wants to type, immediately — no click required.
  await expectEditorFocused(page);
  await page.keyboard.type("Hello");
  await expect(page.locator("#a11yDocument")).toContainText("Hello");
  await expect(page.locator("#documentStateText")).toHaveText("Edited");

  // It is a real word-processing document, not a text dump: the style registry is
  // populated, which only happens when the package carried styles.
  const styles = await definedParagraphStyles(page);
  expect(styles.length, "a blank document must arrive with paragraph styles").toBeGreaterThan(0);

  expect(consoleErrors).toEqual([]);
});

test("New blank document is reachable from the command palette as well as the File menu", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await runFromPalette(page, "file.new");
  await expect(page.locator("#docTitle")).toHaveValue("Untitled document.docx");
  expect(consoleErrors).toEqual([]);
});

// New replaces what is on screen exactly as Open does, so it must go through the
// same unsaved-work gate. A "New" that silently discarded the user's edits would
// be data loss they asked for without knowing.
test("New blank document asks before discarding unsaved edits, and Keep editing keeps them", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.type("KEEPME");
  await expect(page.locator("#documentStateText")).toHaveText("Edited");

  await runFromMenu(page, "file", "file.new");
  const prompt = page.locator("#confirmDialog");
  await expect(prompt).toBeVisible();
  await page.locator("#confirmCancel").click();
  await expect(prompt).toBeHidden();

  // The document is still the one the user was editing, and still editable.
  await expect(page.locator("#a11yDocument")).toContainText("KEEPME");
  await expect(page.locator("#docTitle")).not.toHaveValue("Untitled document.docx");

  // Accepting the prompt is what actually replaces it.
  await runFromMenu(page, "file", "file.new");
  await expect(prompt).toBeVisible();
  await page.locator("#confirmAccept").click();
  await expect(page.locator("#docTitle")).toHaveValue("Untitled document.docx");
  await expect(page.locator("#a11yDocument")).not.toContainText("KEEPME");

  expect(consoleErrors).toEqual([]);
});

// The blank package must be a real DOCX, so Save round-trips it without the user
// having to pick a format. If the host had synthesised the document from plain
// text instead, Save would default to .txt and silently throw away every style
// and the section geometry the moment the user pressed it.
test("a new blank document saves as DOCX by default", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await runFromPalette(page, "file.new");

  await expect(page.locator("#saveFormat")).toHaveValue(
    "org.openxmlformats.wordprocessingml.document",
  );

  const download = page.waitForEvent("download");
  await saveDocument(page);
  const file = await download;
  expect(file.suggestedFilename()).toMatch(/\.docx$/);

  expect(consoleErrors).toEqual([]);
});

// Ribbon density: three surfaces for one persisted choice.
//
// The three are the chevron on the tab strip, the palette, and the View menu.
// The first two live in the RIBBON chrome and the third in the compact chrome
// (docs/122), so the ribbon assertions are made where the ribbon is on screen —
// asserting `.ribbon` state straight after a compact-chrome menu click would be
// asserting about a hidden element.
test("the compact-ribbon choice is reachable from the chevron, the palette and View, and all three agree", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const ribbon = page.locator(".ribbon");
  const band = page.locator(".ribbon-body");
  const chevron = page.locator("#ribbonViewToggle");
  await expect(band).toBeVisible();

  // From the chevron, which used to be the only route.
  await chevron.click();
  await expect(ribbon).toHaveClass(/is-collapsed/);
  await expect(band).toBeHidden();
  await expect(chevron).toHaveAttribute("aria-expanded", "false");

  // The View menu row reads back the current state, so it is a switch rather
  // than an action whose effect the user has to remember — and it agrees with the
  // chevron, or the two controls are two independent switches for one preference.
  await openAppMenu(page, "view");
  await expect(
    page.locator('#appMenuPopover .app-menu-item[data-command="view.compactRibbon"]'),
  ).toContainText("Compact ribbon: on");
  await page.keyboard.press("Escape");
  await page.locator("#modeRibbon").click();

  // From the palette, back off.
  await runFromPalette(page, "view.compactRibbon");
  await expect(band).toBeVisible();
  await expect(chevron).toHaveAttribute("aria-expanded", "true");

  // And from the View menu, on again — the third surface really runs it.
  await runFromMenu(page, "view", "view.compactRibbon");
  await page.locator("#modeRibbon").click();
  await expect(ribbon).toHaveClass(/is-collapsed/);
  await runFromPalette(page, "view.compactRibbon");
  await expect(band).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

test("the compact-ribbon choice survives a reload", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await page.locator("#ribbonViewToggle").click();
  await expect(page.locator(".ribbon")).toHaveClass(/is-collapsed/);

  await gotoEditor(page);
  await expect(
    page.locator(".ribbon"),
    "the ribbon-density preference must be remembered, like every other editor preference",
  ).toHaveClass(/is-collapsed/);
  await expect(page.locator(".ribbon-body")).toBeHidden();

  // Put it back so the preference does not leak into another spec sharing the
  // browser profile.
  await runFromPalette(page, "view.compactRibbon");
  await expect(page.locator(".ribbon-body")).toBeVisible();

  expect(consoleErrors).toEqual([]);
});
