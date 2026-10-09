// A disabled control says why, in the SAME sentence the palette gives for the
// same command (`SKILL` §10: never a dead control).
//
// Restart and Continue numbering, the contextual Table tab, Insert ▸ Link and the
// RTF export tile all greyed out with nothing to say, while the palette already
// carried a reason for each (`desk-02-tab-home`, `desk-02-tab-table`,
// `desk-03-file-pane-02-export`). Each assertion below reads the reason from the
// palette row and requires the control to say the same thing — so the two
// surfaces cannot drift — and then checks the control gets its own title back
// once the precondition is met, because a reason that outlives its cause is a
// second kind of lie.
import {
  test,
  expect,
  MOD,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  openFilePage,
  shortcutHint,
} from "./fixtures.mjs";

/** The palette's reason for a disabled command, read from its row. */
async function paletteReason(page, commandId, query) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill(query);
  const row = page.locator(`#cmdList .cmd-item[data-command-id="${commandId}"]`);
  await expect(row).toBeDisabled();
  const reason = await row.locator(".cmd-hint").textContent();
  await page.keyboard.press("Escape");
  expect(reason, `${commandId} has no reason in the palette either`).toBeTruthy();
  return reason;
}

test("Restart and Continue numbering say why they are unavailable, as the palette does", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("Numbered item");
  const restart = page.locator("#restartList");
  const resume = page.locator("#continueList");

  // Not in a list at all.
  await expect(restart).toBeDisabled();
  await expect(restart).toHaveAttribute("title", await paletteReason(page, "paragraph.list.restart", "restart numbering"));
  await expect(resume).toHaveAttribute("title", await paletteReason(page, "paragraph.list.continue", "continue numbering"));

  // In the document's first numbered list: Restart is live with its own title,
  // and Continue says the OTHER reason — there is nothing earlier to continue.
  await page.locator("#numberedList").click();
  await expect(restart).toBeEnabled();
  await expect(restart).toHaveAttribute("title", "Restart numbering at 1");
  await expect(resume).toBeDisabled();
  await expect(resume).toHaveAttribute("title", "There is no earlier numbered list to continue");
  await expect(resume).toHaveAttribute("title", await paletteReason(page, "paragraph.list.continue", "continue numbering"));
  expect(consoleErrors).toEqual([]);
});

test("the Table tab and Insert ▸ Link say why they are unavailable, and stop saying it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const tab = page.locator("#tabTable");
  await expect(tab).toBeDisabled();
  await expect(tab).toHaveAttribute("title", "Place the caret in a table");

  const link = page.locator("#insertLinkBtn");
  await expect(link).toBeDisabled();
  await expect(link).toHaveAttribute("title", await paletteReason(page, "insert.link", "add or edit link"));

  // With text selected, Link is live and its own title — chord included — is back.
  await page.keyboard.type("LINKME");
  for (let i = 0; i < "LINKME".length; i++) await page.keyboard.press("Shift+ArrowLeft");
  await expect(link).toBeEnabled();
  await expect(link).toHaveAttribute("title", `Add or edit link (${shortcutHint("⌘K")})`);

  // In a table the tab is live, and the reason does not linger as its tooltip.
  await page.keyboard.press("ArrowRight");
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(tab).toBeEnabled();
  expect(await tab.getAttribute("title")).toBeNull();
  expect(consoleErrors).toEqual([]);
});

test("the RTF export tile says why it is unavailable, and no live tile claims it cannot write", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const reason = await paletteReason(page, "file.export.rtf", "rtf");
  await openFilePage(page);
  await page.locator('#filePageBody [data-file-pane="export"]').click();
  const tiles = page.locator("#filePageDetail .file-format-tile");
  const disabled = tiles.filter({ has: page.locator(":scope:disabled") });
  await expect(page.locator("#filePageDetail .file-format-tile:disabled")).toHaveCount(1);
  await expect(page.locator("#filePageDetail .file-format-tile:disabled")).toHaveAttribute("title", reason);
  // Every row carries a reason for the day it is unavailable; only a DISABLED
  // tile may show it.
  const liveTitles = await page
    .locator("#filePageDetail .file-format-tile:enabled")
    .evaluateAll((nodes) => nodes.map((node) => node.getAttribute("title")).filter(Boolean));
  expect(liveTitles).toEqual([]);
  expect(await tiles.count()).toBeGreaterThan(await disabled.count());
  expect(consoleErrors).toEqual([]);
});
