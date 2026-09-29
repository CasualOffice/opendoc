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
  runFilePageCommand,
  useCompactChrome,
} from "./fixtures.mjs";
import { APP_MENU_SECTIONS } from "../../src/command_taxonomy.mjs";

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

  const MENUS = ["file", "edit", "view", "insert", "format", "table", "references", "review"];
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

  await runFilePageCommand(page, "help.about");
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

test("Page setup is on the File surface, where Docs puts it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // There is no Tools menu to keep it out of any more — the whole menu is gone
  // (docs/122). What is worth pinning is that File HAS it, in both renderings of
  // the File roster.
  expect(await menuCommandIds(page, "file")).toContain("layout.pageSetup");
  await page.locator("#modeRibbon").click();
  await page.locator("#tabFile").click();
  // What the page OFFERS, not how it renders the row. The two File surfaces
  // render one roster two ways — a dropdown runs each command, the page may
  // open a pane instead — and `data-covers` exists precisely so a parity test
  // can see through that (`command_menu.mjs`, `categoryRow`). Page setup became
  // a pane rather than a modal over the page, so it is now a category row like
  // Settings and Document properties; asserting `data-command` was asserting
  // the mechanism, and it broke on a change that did not remove anything.
  expect(
    await page.locator("#filePageBody .file-page-item").evaluateAll((rows) =>
      rows.flatMap((r) => [r.dataset.command, ...(r.dataset.covers ?? "").split(" ")]),
    ),
  ).toContain("layout.pageSetup");
  await page.keyboard.press("Escape");

  // And it still opens the dialog from there — a moved row that does not run
  // is worse than a buried one.
  await runAppMenuCommand(page, "file", "layout.pageSetup");
  await expect(page.locator("#pageSetupMenu")).toBeVisible();
  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

// ---- The bands, in the browser ---------------------------------------------
//
// The owner's report was that the menus are flat lists of commands and that
// grouping is necessary — "in file menus in compact mode … basically menus like
// File, Edit, View". The structural half is `menu_taxonomy.test.mjs`: every
// band is named, non-empty and lists nothing twice. This is the half only a
// browser can answer — that the bands actually reach the screen, that each one
// is announced by NAME rather than as an anonymous rule, and that the keyboard
// still walks rows and only rows.
//
// Every assertion here is written against the guarantee and not against a
// count. Nothing below names a number of rules, a number of bands or a band's
// membership by hand: a menu may be regrouped tomorrow and these stay green,
// while a menu that goes back to one undifferentiated run cannot.

/** Every menu row on screen, with the band it belongs to. */
async function rowsWithGroups(page, menu) {
  await openAppMenu(page, menu);
  const rows = await page.$$eval("#appMenuPopover .app-menu-item", (items) =>
    items.map((item) => {
      const group = item.closest('[role="group"]');
      return {
        id: item.dataset.command,
        group: group?.dataset.group ?? null,
        name: group?.getAttribute("aria-label") ?? null,
      };
    }),
  );
  await page.keyboard.press("Escape");
  return rows;
}

test("every menu row is inside a band with a translated name", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await useCompactChrome(page);
  await clickIntoFirstPage(page);

  const loose = [];
  const unnamed = [];
  const untranslated = [];
  for (const menu of Object.keys(APP_MENU_SECTIONS)) {
    const rows = await rowsWithGroups(page, menu);
    expect(rows.length, `the ${menu} menu rendered nothing`).toBeGreaterThan(0);
    for (const row of rows) {
      if (!row.group) loose.push(`${menu}: ${row.id}`);
      else if (!row.name) unnamed.push(`${menu}: ${row.group}`);
      // `t()` renders the KEY when a catalogue cannot answer it, so a band
      // named `menuGroup.clipboard` is one a screen reader reads out as a
      // dotted identifier. That is the failure mode this catches, and it is
      // invisible to a check that only asks whether the name is non-empty.
      else if (row.name === row.group) untranslated.push(`${menu}: ${row.group}`);
    }
  }
  expect(
    loose,
    "a menu row outside every band — it belongs to no group a reader can be told about",
  ).toEqual([]);
  expect(unnamed, "a band with no accessible name is a line that says nothing").toEqual([]);
  expect(untranslated, "these bands are announced as their own catalogue keys").toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("each menu renders the bands the taxonomy declares, in order", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await useCompactChrome(page);
  await clickIntoFirstPage(page);

  // Band membership proved through the DOM rather than trusted from the data:
  // a renderer that dropped the grouping and emitted one flat run would satisfy
  // the unit test and fail here.
  for (const [menu, sections] of Object.entries(APP_MENU_SECTIONS)) {
    const rows = await rowsWithGroups(page, menu);
    const keyFor = new Map();
    for (const section of sections) for (const id of section.ids) keyFor.set(id, section.nameKey);
    const misfiled = rows.filter((row) => row.group !== keyFor.get(row.id));
    expect(
      misfiled,
      `${menu}: rows rendered under a band the taxonomy did not put them in`,
    ).toEqual([]);

    // A band whose rows the registry does not offer prints nothing at all — no
    // heading over blank space and no rule with nothing under it — so what is
    // on screen is exactly the bands with at least one live row, in order.
    const onScreen = [...new Set(rows.map((row) => row.group))];
    const populated = sections.filter((section) =>
      section.ids.some((id) => rows.some((row) => row.id === id)),
    );
    expect(onScreen).toEqual(populated.map((section) => section.nameKey));
  }
  expect(consoleErrors).toEqual([]);
});

test("arrow, Home and End walk rows only — a band boundary is never a stop", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await useCompactChrome(page);
  await clickIntoFirstPage(page);

  // Format has the most bands and the most rows, so walking it end to end
  // crosses every kind of boundary this change introduced.
  await openAppMenu(page, "format");
  const rowCount = await page
    .locator("#appMenuPopover .app-menu-item:not([disabled]), #appMenuPopover .app-menu-item-parent")
    .count();
  expect(rowCount).toBeGreaterThan(8);

  /** What the keyboard is actually on: the role it carries and whether it is a
   *  row. A band container caught in the walk answers `"group"` here.
   *
   *  A SUBMENU PARENT counts as a row. It is a focusable `menuitem` that opens
   *  a flyout — a destination, not a boundary — and the guarantee this test
   *  holds is that the walk never lands on a group CONTAINER, which is still
   *  asserted below. Before submenus every row wore one class, so the class was
   *  a fair proxy for "is a row"; it is not any more.  */
  const focused = () =>
    page.evaluate(() => {
      const el = document.activeElement;
      const classes = el?.classList;
      return {
        role: el?.getAttribute("role") ?? null,
        isRow: !!(classes?.contains("app-menu-item") || classes?.contains("app-menu-item-parent")),
        disabled: el?.disabled ?? null,
      };
    });

  // One full lap plus two, so the wrap at each end is walked as well as the
  // middle: the boundary between the last band and the first is the one a
  // modulo bug lands on.
  for (let step = 0; step < rowCount + 2; step += 1) {
    expect(await focused(), `ArrowDown step ${step} left the rows`).toEqual({
      role: "menuitem",
      isRow: true,
      disabled: false,
    });
    await page.keyboard.press("ArrowDown");
  }
  for (let step = 0; step < rowCount + 2; step += 1) {
    await page.keyboard.press("ArrowUp");
    expect(await focused(), `ArrowUp step ${step} left the rows`).toEqual({
      role: "menuitem",
      isRow: true,
      disabled: false,
    });
  }

  // Home and End jump ACROSS bands in one press, so they are the two most
  // likely to land on a container.
  await page.keyboard.press("End");
  expect(await focused()).toEqual({ role: "menuitem", isRow: true, disabled: false });
  await page.keyboard.press("Home");
  expect(await focused()).toEqual({ role: "menuitem", isRow: true, disabled: false });

  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("the File page prints the same bands the File dropdown announces", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.locator("#modeRibbon").click();
  await page.locator("#tabFile").click();

  const printed = await page.$$eval("#filePageBody [role='group']", (groups) =>
    groups.map((group) => ({
      key: group.dataset.group,
      heading: group.querySelector("h3")?.textContent ?? null,
      labelled: group.getAttribute("aria-labelledby"),
    })),
  );
  expect(printed.length).toBeGreaterThan(3);
  for (const group of printed) {
    expect(group.heading, `${group.key} printed no heading`).toBeTruthy();
    expect(group.heading).not.toBe(group.key);
    // Named BY the heading rather than beside it: a visible heading plus an
    // `aria-label` repeating it is the same name written twice, and two copies
    // of one name drift.
    expect(group.labelled, `${group.key} has a heading it does not point at`).toBeTruthy();
  }
  await page.keyboard.press("Escape");

  // One declaration, two renderings: the dropdown announces a band, the page
  // prints it, and the words are the same because both read `t(nameKey)`.
  await useCompactChrome(page);
  const dropdown = await rowsWithGroups(page, "file");
  expect([...new Set(dropdown.map((row) => row.group))]).toEqual(printed.map((g) => g.key));
  const announced = new Map(dropdown.map((row) => [row.group, row.name]));
  for (const group of printed) expect(announced.get(group.key)).toBe(group.heading);
  expect(consoleErrors).toEqual([]);
});

test("References is reachable from the compact chrome, not only from the ribbon", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await useCompactChrome(page);
  await clickIntoFirstPage(page);

  // The ribbon is hidden in this chrome, so before the References menu existed
  // the table of contents, captions and cross-references were reachable here
  // only by typing their names into the command palette — a single-surface
  // capability in the chrome whose ONLY axis is this bar.
  await expect(page.locator("#ribbon")).toBeHidden();
  const ids = await menuCommandIds(page, "references");
  for (const id of [
    "reference.tableOfContents",
    "reference.caption",
    "reference.crossReference",
    "insert.footnote",
  ]) {
    expect(ids, `${id} has no References menu row`).toContain(id);
  }

  // And a row RUNS rather than being a label that closes the menu: the caption
  // dialog opening is the engine-side proof.
  await runAppMenuCommand(page, "references", "reference.caption");
  await expect(page.locator("#captionDialog")).toBeVisible();
  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});
