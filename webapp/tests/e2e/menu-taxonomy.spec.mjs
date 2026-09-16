// The behavioural half of the menu-bar contract. `webapp/tests/menu_taxonomy.test.mjs`
// checks the structure as text — one home per command, every menu has a button,
// the Table menu covers every table command. This checks the part only a
// browser can: that those rows actually render, gate correctly, and run the
// same engine action their other surfaces run.
//
// docs/105 UX-012 (no Table menu) and UX-014 (taxonomy faults).
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  openAppMenu,
  runAppMenuCommand,
} from "./fixtures.mjs";

/** Command ids offered by a menu, in render order. */
async function menuCommandIds(page, menu) {
  await openAppMenu(page, menu);
  const ids = await page.$$eval("#appMenuPopover .app-menu-item", (rows) =>
    rows.map((r) => r.dataset.command),
  );
  await page.keyboard.press("Escape");
  return ids;
}

test("no command is offered by two different menus", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const MENUS = ["file", "edit", "view", "insert", "format", "table", "review", "tools", "help"];
  const home = new Map();
  for (const menu of MENUS) {
    for (const id of await menuCommandIds(page, menu)) {
      if (!home.has(id)) home.set(id, []);
      home.get(id).push(menu);
    }
  }

  // Eight ids used to appear twice — the three review modes, review.toggle,
  // view.showChanges, review.comment, layout.paragraph and file.properties.
  // Reaching a command from several SURFACES is required here; repeating it
  // inside the bar is what made the menus stop being a map.
  const repeated = [...home.entries()]
    .filter(([, menus]) => menus.length > 1)
    .map(([id, menus]) => `${id} → ${menus.join(", ")}`);
  expect(repeated).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

/** A 2x2 table with the caret inside it, via the Insert ribbon's grid picker. */
async function insertTwoByTwoTable(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
}

test("the Table menu lists its commands with the caret outside a table, greyed", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const ids = await menuCommandIds(page, "table");
  // Building the menu only from a live table context opened an EMPTY popover
  // with the caret in a paragraph, which tells the user the editor cannot edit
  // tables — the same thing having no Table menu said. Word and Docs grey the
  // rows and keep them.
  for (const id of [
    "table.insert.rowAbove",
    "table.delete.row",
    "table.select.table",
    "table.merge",
    "table.properties",
  ]) {
    expect(ids, `${id} must be listed even outside a table`).toContain(id);
  }

  await openAppMenu(page, "table");
  const row = page.locator('#appMenuPopover .app-menu-item[data-command="table.insert.rowAbove"]');
  await expect(row).toBeDisabled();
  // Disabled without a reason is a dead end; the row must say what to do.
  await expect(row).toHaveAttribute("title", /caret in a table/i);

  // Labels must not stutter: the palette prefixes these with "Table:" because
  // it is one flat global list; inside a menu called Table that reads twice.
  const labels = await page.$$eval("#appMenuPopover .app-menu-item-label", (els) =>
    els.map((e) => e.textContent.trim()),
  );
  expect(labels.filter((l) => l.startsWith("Table:"))).toEqual([]);
  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("a Table menu row runs the real engine action on a real table", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwoTable(page);

  // With the caret in the table the same row must now be live...
  await openAppMenu(page, "table");
  await expect(
    page.locator('#appMenuPopover .app-menu-item[data-command="table.insert.rowAbove"]'),
  ).toBeEnabled();
  await page.keyboard.press("Escape");

  // ...and running it must change the DOCUMENT, not merely close the menu.
  // Selecting the whole table paints one rect per cell, so a 2x2 selects 4 and
  // a 3x2 selects 6. A menu row wired to a no-op — a label with no `run`, the
  // thing a hand-written menu list invites — leaves this at 4.
  const selection = page.locator(".table-cell-selection");
  await runAppMenuCommand(page, "table", "table.select.table");
  await expect(selection).toHaveCount(4);

  await runAppMenuCommand(page, "table", "table.insert.rowAbove");
  await runAppMenuCommand(page, "table", "table.select.table");
  await expect(selection).toHaveCount(6);

  expect(consoleErrors).toEqual([]);
});

test("Help ▸ About opens and reports a version the engine supplied", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  await runAppMenuCommand(page, "help", "help.about");
  const dialog = page.locator("#aboutDialog");
  await expect(dialog).toBeVisible();

  // The version is filled from `engineVersion()`, compiled from the crate
  // manifest. A placeholder or a hardcoded string here is the EV-002 failure —
  // a number on a user-facing surface that no committed artifact backs.
  const version = page.locator("#aboutVersion");
  await expect(version).toBeVisible();
  await expect(version).toHaveText(/^\d+\.\d+\.\d+/);

  // It must say what the licence is: the licence IS the product's wedge, and
  // "which licence is this under" had no answer anywhere in the UI.
  await expect(dialog).toContainText("Apache-2.0");

  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("Page setup is in File, where Docs puts it, and not in Tools", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  expect(await menuCommandIds(page, "file")).toContain("layout.pageSetup");
  expect(await menuCommandIds(page, "tools")).not.toContain("layout.pageSetup");

  // And it still opens the dialog from there — a moved row that does not run
  // is worse than a buried one.
  await runAppMenuCommand(page, "file", "layout.pageSetup");
  await expect(page.locator("#pageSetupMenu")).toBeVisible();
  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});
