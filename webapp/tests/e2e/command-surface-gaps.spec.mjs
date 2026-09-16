// Five capabilities that existed only as ribbon chrome, from docs/104:
//
//   HF-147  font family and font size had no command id at all
//   HF-148  `insert.table` WAS a 3x3 table — the size was baked into the id
//   HF-149  the nine zoom presets had no ids
//   HF-150  the underline-style menu had no ids
//   HF-151  nothing in the product listed the keyboard shortcuts
//
// The shared defect is the one docs/104 keeps recording: a capability reachable
// from exactly one surface. Each test here drives the capability from the
// palette and then checks the DOCUMENT changed, because a palette row that
// merely exists would be worse than none.
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

test("an exact font size is reachable from the palette and lands on the text", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+a`);

  await runFromPalette(page, "size point 28", "Font size: 28 pt");
  await expect(page.locator("#fontSize")).toHaveValue("28");
  expect(consoleErrors).toEqual([]);
});

test("an exact font face is reachable from the palette and lands on the text", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+a`);

  await runFromPalette(page, "family georgia", "Font: Georgia");
  await expect(page.locator("#fontFamilyLabel")).toHaveText("Georgia");
  expect(consoleErrors).toEqual([]);
});

test("a zoom preset is a command, not just a menu row", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  const zoom = page.locator("#zoom");
  await expect(zoom).not.toHaveValue("150%");

  await runFromPalette(page, "zoom: 150", "Zoom: 150%");
  await expect(zoom).toHaveValue("150%");

  // Fit modes too — they are the two rows a keyboard user most wants and had no
  // id of their own either.
  await runFromPalette(page, "zoom fit width", "Zoom: Fit width");
  await expect(zoom).toHaveValue("Fit width");
  expect(consoleErrors).toEqual([]);
});

test("an underline style is a command, and it really changes the style", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+a`);

  await runFromPalette(page, "underline wavy", "Underline: Wavy");
  await expect(page.locator("#underline")).toHaveAttribute("data-underline-style", "wavy");
  expect(consoleErrors).toEqual([]);
});

test("Insert table no longer claims to be 3x3, and opens the size picker", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // The old id ran a 3x3 table and said so in its own label, so no caller could
  // ask for any other size. It now opens the grid the ribbon opens.
  await runFromPalette(page, "insert table", "Insert table…");
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="4"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();

  // The pages are painted, not DOM tables, so the size is read from the
  // contextual Table tab's own readout of the caret's table.
  await page.locator("#tabTable").click();
  await expect(page.locator("#tableContext")).toContainText("2×4 table");
  expect(consoleErrors).toEqual([]);
});

test("the keyboard shortcuts reference lists real chords, grouped, with no Apple glyph on a PC", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await runFromPalette(page, "keyboard shortcuts", "Keyboard shortcuts");
  await expect(page.locator("#shortcutsDialog")).toBeVisible();

  const reference = await page.evaluate(() => ({
    groups: [...document.querySelectorAll(".shortcuts-group h3")].map((h) => h.textContent),
    rows: [...document.querySelectorAll(".shortcuts-row")].map((r) => ({
      label: r.children[0].textContent,
      keys: r.children[1].textContent,
    })),
  }));

  // Caret movement has no command behind it, so it appears in no menu and in no
  // palette; if the reference is generated from the registry alone it silently
  // omits the half of the keymap people most need written down.
  expect(reference.groups).toContain("Moving around");
  expect(reference.groups.length).toBeGreaterThan(3);
  expect(reference.rows.length).toBeGreaterThan(15);

  // Every row must carry keys, or it is a label pretending to be a shortcut.
  for (const row of reference.rows) {
    expect(row.label.trim(), JSON.stringify(row)).not.toBe("");
    expect(row.keys.trim(), JSON.stringify(row)).not.toBe("");
  }

  // A shortcut the product genuinely answers to, spelled for this platform.
  const save = reference.rows.find((r) => r.label === "Save");
  expect(save).toBeTruthy();
  expect(save.keys).toBe(process.platform === "darwin" ? "⌘S" : "Ctrl+S");

  if (process.platform !== "darwin") {
    for (const row of reference.rows) {
      expect(/[⌘⌥⇧⌃]/u.test(row.keys), `${row.label} shows an Apple glyph: ${row.keys}`).toBe(
        false,
      );
    }
  }
  expect(consoleErrors).toEqual([]);
});
