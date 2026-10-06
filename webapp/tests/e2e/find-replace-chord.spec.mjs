// Ctrl+H opens Find and replace with the replacement field ready.
//
// It is the chord in Word, Google Docs and ONLYOFFICE, and here it did nothing:
// the Replace button was declared as a second face of `edit.find`, so there was
// no command for a chord to name, and its tooltip advertised ⌘F. The Mac chord
// is ⌘⇧H (Google Docs'), because ⌘H is the operating system's Hide and never
// reaches a page — so the keystroke and every expectation below are derived per
// platform rather than written as one literal.
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

const APPLE = process.platform === "darwin";
/** The chord as the keymap declares it on the platform running the suite. */
const REPLACE_CHORD = APPLE ? "⌘⇧H" : "⌘H";
/** …and as a Playwright keystroke. `MOD` keeps it portable (`chord_portability`). */
const REPLACE_KEYS = APPLE ? `${MOD}+Shift+h` : `${MOD}+h`;

test("Ctrl+H opens Find and replace on the replacement, seeded with the selection", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("REPLACEME");
  for (let i = 0; i < "REPLACEME".length; i++) await page.keyboard.press("Shift+ArrowLeft");

  await page.keyboard.press(REPLACE_KEYS);
  await expect(page.locator("#findPanel")).toBeVisible();
  await expect(page.locator("#findInput")).toHaveValue("REPLACEME");
  // The point of the chord: the keyboard is where the NEW text goes.
  await expect(page.locator("#replaceInput")).toBeFocused();
  await page.keyboard.type("SWAPPED");
  await expect(page.locator("#replaceInput")).toHaveValue("SWAPPED");

  // And it is the real panel: Replace all writes the document.
  await page.locator("#replaceAll").click();
  await expect(page.locator("#a11yDocument")).toContainText("SWAPPED");
  await expect(page.locator("#a11yDocument")).not.toContainText("REPLACEME");
  expect(consoleErrors).toEqual([]);
});

test("with nothing to replace yet, Ctrl+H starts on the Find field", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator("#findInput").evaluate((input) => (input.value = ""));
  await page.keyboard.press(REPLACE_KEYS);
  await expect(page.locator("#findPanel")).toBeVisible();
  await expect(page.locator("#replaceInput")).toBeVisible();
  await expect(page.locator("#findInput")).toBeFocused();
  expect(consoleErrors).toEqual([]);
});

test("Replace advertises its chord on the ribbon, in the palette and in Shortcuts, and the button opens it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const hint = shortcutHint(REPLACE_CHORD);

  // The ribbon control. Its chord is read from the keymap per platform, not
  // from markup, which said ⌘F.
  await expect(page.locator("#replaceBtn")).toHaveAttribute("title", `Replace (${hint})`);
  await expect(page.locator("#replaceBtn")).toHaveAttribute("data-command", "edit.replace");

  // The palette row.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("replace");
  const row = page.locator('#cmdList .cmd-item[data-command-id="edit.replace"]');
  await expect(row.locator(".cmd-hint")).toHaveText(hint);
  await page.keyboard.press("Escape");

  // File ▸ Shortcuts.
  await openFilePage(page);
  await page.locator('#filePageBody [data-file-pane="shortcuts"]').click();
  const reference = page.locator("#shortcutsDialog .shortcuts-row").filter({ hasText: /^Replace/ });
  await expect(reference.locator(".shortcuts-keys")).toHaveText(hint);
  await page.keyboard.press("Escape");

  // The button runs the same command.
  await page.locator("#tabHome").click();
  await page.locator("#replaceBtn").click();
  await expect(page.locator("#findPanel")).toBeVisible();
  expect(consoleErrors).toEqual([]);
});
