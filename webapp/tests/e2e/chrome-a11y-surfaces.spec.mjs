// Four accessibility defects from docs/104-HOTFIX-TRACKER.md that each made a
// working feature unusable without a mouse or without sight, plus the floating
// toolbar's missing pressed state. They share a file because they share a
// cause: state and structure were published to the eye and to one surface only.
//
//   HF-029  the status line was the only error channel and was not a live region
//   HF-031  the palette announced nothing while arrowing through results
//   HF-032  the insert-table grid was pointer-only and had 80 unnamed buttons
//   HF-034  Open could not be focused or activated by keyboard. The control has
//           since moved off the top bar into File ▸ Open (it duplicated the menu),
//           so the contract is now that the MENU route is fully keyboard-driven —
//           the defect was never about the button, it was about there being no
//           keyboard way into a freshly loaded editor.
//   HF-072  the floating selection toolbar never showed Bold/Italic/Underline state
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

// ---- HF-034 -----------------------------------------------------------------

test("Open is reachable and activatable by keyboard alone, with no pointer", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // The original defect was a `<label>` wrapping a hidden input: it takes no
  // focus and has no keydown wiring, so a freshly loaded editor had no keyboard
  // route in at all. Open now lives in the File menu, so the whole route has to
  // work from the keyboard — the menu button, the menu, and the row.
  const fileMenu = page.locator('.app-menu-button[data-menu="file"]');
  await expect(fileMenu).toBeVisible();
  expect(await fileMenu.evaluate((el) => el.tagName)).toBe("BUTTON");

  await fileMenu.focus();
  await expect(fileMenu).toBeFocused();

  await page.keyboard.press("Enter");
  const row = page.locator('#appMenuPopover .app-menu-item[data-command="file.open"]');
  await expect(row).toBeVisible();
  await expect(row).toBeEnabled();

  // Focus must land INSIDE the menu on open, or arrowing to the row is
  // impossible and the menu is a pointer-only surface wearing menu semantics.
  await expect(page.locator("#appMenuPopover")).toContainText("Open");
  const focusedInMenu = await page.evaluate(
    () => !!document.activeElement?.closest("#appMenuPopover"),
  );
  expect(focusedInMenu, "opening the File menu must move focus into it").toBe(true);

  // Enter on the row opens the real file picker — the whole point of the
  // control, and the half that a `tabindex` alone would not have delivered.
  const chooser = page.waitForEvent("filechooser");
  await row.focus();
  await page.keyboard.press("Enter");
  expect(await chooser).toBeTruthy();

  // The input itself stays out of the tab order rather than becoming a second,
  // dead stop.
  await expect(page.locator("#file")).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

// ---- HF-029 -----------------------------------------------------------------

test("a refused edit is announced assertively and an ordinary message politely", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const polite = page.locator("#statusLiveRegion");
  const assertive = page.locator("#statusAlertRegion");
  await expect(assertive).toHaveAttribute("aria-live", "assertive");
  await expect(polite).toHaveAttribute("aria-live", "polite");

  // A failure: Viewing mode refuses the keystroke. Painting "read-only" into the
  // footer said nothing at all to a screen reader before.
  await setReviewMode(page, "viewing");
  await clickIntoFirstPage(page);
  await page.keyboard.type("X");
  await expect(page.locator("#status")).toContainText("read-only");
  await expect(assertive).toContainText("read-only");
  await expect(polite).toHaveText("");

  // A routine message goes the other way, so an interruption stays reserved for
  // something that actually went wrong.
  await setReviewMode(page, "editing");
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTextBoxBtn").click();
  await expect(page.locator("#status")).toContainText("Text box added");
  await expect(polite).toContainText("Text box added");
  await expect(assertive).toHaveText("");

  expect(consoleErrors).toEqual([]);
});

// ---- HF-031 -----------------------------------------------------------------

test("the command palette announces the option the arrow keys land on", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.keyboard.press(`${MOD}+Shift+P`);
  const palette = page.locator("#cmdPalette");
  await expect(palette).toBeVisible();

  const input = page.locator("#cmdInput");
  await expect(input).toHaveAttribute("role", "combobox");
  await expect(input).toHaveAttribute("aria-controls", "cmdList");
  await expect(input).toHaveAttribute("aria-autocomplete", "list");
  await expect(input).toHaveAttribute("aria-expanded", "true");

  await input.fill("insert");
  await expect(page.locator("#cmdList .cmd-item")).not.toHaveCount(0);

  // The highlight and the announcement are the same selection, not two.
  const selectedId = () => page.locator("#cmdList .cmd-item.sel").getAttribute("id");
  const first = await selectedId();
  expect(first).toBeTruthy();
  await expect(input).toHaveAttribute("aria-activedescendant", first);
  await expect(page.locator("#cmdList .cmd-item[aria-selected='true']")).toHaveCount(1);
  await expect(page.locator("#cmdList .cmd-item.sel")).toHaveAttribute("aria-selected", "true");

  // Arrowing moves both of them together — silence here was the whole defect.
  await input.press("ArrowDown");
  const second = await selectedId();
  expect(second).not.toBe(first);
  await expect(input).toHaveAttribute("aria-activedescendant", second);
  await expect(page.locator("#cmdList .cmd-item[aria-selected='true']")).toHaveCount(1);

  // Options must not be tab stops: focus stays in the query field, which is what
  // `aria-activedescendant` exists to make possible.
  await expect(input).toBeFocused();
  expect(await page.locator("#cmdList .cmd-item[tabindex='0']").count()).toBe(0);

  // No matches: nothing is active, and the combobox says so.
  await input.fill("zzzznotacommand");
  await expect(page.locator(".cmd-empty")).toBeVisible();
  await expect(input).toHaveAttribute("aria-expanded", "false");
  expect(await input.getAttribute("aria-activedescendant")).toBeNull();

  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

// ---- HF-032 -----------------------------------------------------------------

test("the insert-table grid is a named, arrow-navigable grid, not 80 anonymous buttons", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();

  // Structure and names: a grid of rows whose cells say what they insert.
  await expect(page.locator("#gridPicker")).toHaveAttribute("role", "grid");
  await expect(page.locator("#gridPicker [role='row']")).toHaveCount(8);
  await expect(page.locator('.gc[data-r="3"][data-c="4"]')).toHaveAttribute(
    "aria-label",
    "4 by 3 table",
  );

  // One tab stop for the whole grid (roving tabindex), and opening the popover
  // puts the keyboard on it instead of leaving it behind the focus ring.
  expect(await page.locator("#gridPicker .gc[tabindex='0']").count()).toBe(1);
  await expect(page.locator('.gc[data-r="1"][data-c="1"]')).toBeFocused();

  // Arrows navigate and preview: two down and three right is a 4 × 3 table.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
  await expect(page.locator('.gc[data-r="3"][data-c="4"]')).toBeFocused();
  await expect(page.locator("#gridLabel")).toHaveText("4 × 3");

  // The edge does not wrap onto a different size.
  await page.keyboard.press("End");
  await expect(page.locator('.gc[data-r="3"][data-c="10"]')).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator('.gc[data-r="3"][data-c="10"]')).toBeFocused();
  await page.keyboard.press("Home");
  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");

  // Enter inserts, through the same path the pointer uses.
  await page.keyboard.press("Enter");
  await expect(page.locator("#insertTableMenu")).toBeHidden();
  await expect(page.locator("#tabTable")).toBeEnabled();

  expect(consoleErrors).toEqual([]);
});

// ---- HF-072 -----------------------------------------------------------------

test("the floating selection toolbar reflects the selection's format", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("SELTOOLBAR");
  for (let i = 0; i < "SELTOOLBAR".length; i++) await page.keyboard.press("Shift+ArrowLeft");

  const selBold = page.locator('#selToolbar [data-fmt="bold"]');
  const selItalic = page.locator('#selToolbar [data-fmt="italic"]');
  await expect(page.locator("#selToolbar")).toBeVisible();
  await expect(selBold).toHaveAttribute("aria-pressed", "false");

  // Bold the selection from the ribbon. The bar floating over the selection is
  // the one the user is looking at, so it has to agree with the distant ribbon
  // button — otherwise clicking its B removes bold instead of applying it.
  await page.locator("#bold").click();
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "true");
  await expect(selBold).toHaveAttribute("aria-pressed", "true");
  await expect(selItalic).toHaveAttribute("aria-pressed", "false");

  // And clicking the bar's own B on already-bold text removes it, rather than
  // the state being decorative.
  await selBold.click();
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "false");
  await expect(selBold).toHaveAttribute("aria-pressed", "false");

  expect(consoleErrors).toEqual([]);
});
