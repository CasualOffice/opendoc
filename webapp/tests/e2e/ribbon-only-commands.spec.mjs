// Ribbon-only capabilities, reachable from the command palette.
//
// Checklist, line spacing and the bullet/numbering galleries existed ONLY as
// controls on the Home ribbon tab. None had an entry in `editorCommands()`, so
// none was reachable from the palette or from any app menu: a user who did not
// already know which button to hunt for could not find them, and neither could
// anyone driving the editor from the keyboard. Docs gives line spacing its own
// top-level submenu; both Word and Docs put the marker galleries behind a
// searchable path.
//
// The palette rows are generated from the popovers' own markup, so what is
// asserted here is that the generated rows really run the same actions the
// ribbon controls run — a row that merely exists would be worse than none.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

async function runFromPalette(page, query, label) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill(query);
  const row = page.locator("#cmdList .cmd-item", { hasText: label }).first();
  await expect(row).toBeVisible();
  await row.click();
  await expect(page.locator("#cmdPalette")).toBeHidden();
}

test("Checklist is reachable from the palette and toggles the caret's paragraph", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await expect(page.locator("#checkList")).toHaveAttribute("aria-pressed", "false");

  await runFromPalette(page, "checklist", "Checklist");

  // The ribbon's own checklist button reports the pressed state, which it reads
  // back from the engine's list style at the caret — so the command drove the
  // same engine path the button drives.
  await expect(page.locator("#checkList")).toHaveAttribute("aria-pressed", "true");

  // One undoable action.
  await page.keyboard.press(`${MOD}+z`);
  await expect(page.locator("#checkList")).toHaveAttribute("aria-pressed", "false");

  expect(consoleErrors).toEqual([]);
});

test("line spacing presets are reachable from the palette and apply to the paragraph", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await runFromPalette(page, "Line spacing: Double", "Line spacing: Double");

  // The spacing popover reflects the engine's line spacing at the caret, so
  // asking the control this command mirrors is what proves the value actually
  // landed on the paragraph rather than the row merely being clickable.
  await page.locator('[data-tab="home"]').click();
  await page.locator("#spacingBtn").click();
  await expect(page.locator('#spacingMenu .spacing-line[data-percent="200"]')).toHaveAttribute(
    "aria-checked",
    "true",
  );

  expect(consoleErrors).toEqual([]);
});

test("bullet styles are reachable from the palette and apply to the list", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Make a bullet list first — a marker style needs a list to apply to.
  await runFromPalette(page, "bullet list", "Bullet list");
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");

  // Choose a specific marker from the palette, then confirm the gallery — the
  // control this command mirrors — reports that exact spec as the current one.
  await runFromPalette(page, "Bullet style: Filled square", "Bullet style: Filled square bullet");

  await page.locator('[data-tab="home"]').click();
  await page.locator("#bulletListMenuBtn").click();
  await expect(
    page.locator('#bulletGalleryMenu .list-gallery-cell[aria-label="Filled square bullet"]'),
  ).toHaveAttribute("aria-checked", "true");

  expect(consoleErrors).toEqual([]);
});

// The generated rows must stay in step with the controls they mirror: every
// preset and every gallery cell gets a command, so adding one to the markup
// cannot leave it unreachable the way these three were.
test("every spacing preset and gallery cell has a palette command", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const expected = await page.evaluate(() => ({
    presets: [...document.querySelectorAll("#spacingMenu .spacing-line")].map((el) =>
      `Line spacing: ${el.textContent.trim()}`,
    ),
    bullets: [...document.querySelectorAll("#bulletGalleryMenu .list-gallery-cell")].map(
      (el) => `Bullet style: ${el.getAttribute("aria-label")}`,
    ),
    numbers: [...document.querySelectorAll("#numberGalleryMenu .list-gallery-cell")].map(
      (el) => `Numbering format: ${el.getAttribute("aria-label")}`,
    ),
  }));
  expect(expected.presets.length).toBeGreaterThan(0);
  expect(expected.bullets.length).toBeGreaterThan(0);
  expect(expected.numbers.length).toBeGreaterThan(0);

  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  for (const label of [...expected.presets, ...expected.bullets, ...expected.numbers]) {
    await page.locator("#cmdInput").fill(label);
    await expect(page.locator("#cmdList .cmd-item", { hasText: label }).first()).toBeVisible();
  }

  expect(consoleErrors).toEqual([]);
});
