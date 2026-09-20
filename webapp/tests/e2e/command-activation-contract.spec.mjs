// THE ACTIVATION CONTRACT: a command a user can see must do something when it
// is clicked, or say why it cannot.
//
// The defect this file exists for was reported as "there is setting in tools..
// i cant open it". Tools ▸ Settings looked completely normal — the row was
// there, it was enabled, it had the right label, the command id resolved and
// its `run` was called — and clicking it did nothing at all. Nothing threw,
// nothing was logged. The panel WAS opened and then closed again inside the
// same click: the menu row's click ran the command, the command opened the
// settings panel, and that same click carried on bubbling to a document-level
// "click outside closes it" listener, which saw a target outside the panel and
// shut it. One event, open and shut, no paint.
//
// That is a CLASS, not one wire (SKILL.md §10): any surface dismissed on the
// `click` phase can be dismissed by the very click that opened it, from any
// surface that activates on click — menu row, palette row, ribbon button.
// Patching `view.settings` would have left the next one to be found by a user.
// So this contract sweeps the whole menu bar and the whole ribbon and asserts
// the guarantee, not the mechanism:
//
//   1. an enabled control, when activated, CHANGES something outside the menu
//      chrome (it is not inert),
//   2. it does not open a surface and close it again in the same activation
//      (it is not dead),
//   3. it throws nothing,
//   4. and a control that cannot act right now is disabled WITH A REASON,
//      never present-and-silent.
//
// Every row and every button is discovered from the live DOM, so a command
// added to a menu or the ribbon tomorrow is covered without touching this file.
import { test, expect, gotoEditor, clickIntoFirstPage, openAppMenu } from "./fixtures.mjs";

const MENUS = ["file", "edit", "view", "insert", "format", "table", "review", "tools", "help"];
const RIBBON_TABS = ["home", "insert", "layout", "references", "view", "review"];

// The only commands this sweep may not activate, and why. Both hand control to
// the operating system and never give it back to the test: `window.print()`
// blocks the renderer until the print dialog is dismissed (see `printDocument`),
// and the two file commands open a native file chooser. Both are covered by
// their own specs (`print.spec.mjs`, `multi-format-io.spec.mjs`). The ids are
// asserted to still exist below, so a skip cannot outlive its command.
const POINTER_UNSAFE = new Map([
  ["file.print", "window.print() blocks the renderer until the OS dialog closes"],
  ["file.open", "opens a native file chooser"],
  ["insert.image", "opens a native file chooser"],
]);

/** Starts recording every DOM change, tagged with the observer batch it arrived
 *  in, so an activation can be examined as one event rather than as a state
 *  difference (a surface that opens and closes leaves no difference at all). */
const startRecording = () => {
  window.__activation = { records: [], batch: 0 };
  const observer = new MutationObserver((records) => {
    const batch = window.__activation.batch++;
    for (const record of records) {
      const el = record.type === "characterData" ? record.target.parentElement : record.target;
      if (!el) continue;
      const name =
        el.id ||
        (typeof el.className === "string" && el.className ? `.${el.className.split(" ")[0]}` : "") ||
        el.tagName;
      // What does NOT count as the command doing something: the menu closing
      // behind it, and the hover chrome the pointer drags along with it. The
      // delayed ribbon tooltip fires wherever the mouse comes to rest after a
      // menu row is clicked, and it rewrites `title`/`data-tip-title` on the
      // control underneath. Counting that trail made two genuinely inert
      // commands look alive in the first run of this sweep — a guard passing
      // for a reason that has nothing to do with what it claims to check.
      const isPointerTrail =
        !!el.closest?.(".ribbon-tooltip") ||
        record.attributeName === "title" ||
        record.attributeName === "data-tip-title";
      window.__activation.records.push({
        batch,
        attribute: record.attributeName ?? null,
        name,
        wasHidden: record.attributeName === "hidden" ? record.oldValue !== null : null,
        isHidden: record.attributeName === "hidden" ? el.hasAttribute("hidden") : null,
        ignored:
          isPointerTrail || !!el.closest?.("#appMenuPopover, .app-menu-bar, .ribbon-tabs"),
      });
    }
  });
  observer.observe(document.documentElement, {
    attributes: true,
    attributeOldValue: true,
    subtree: true,
    childList: true,
    characterData: true,
  });
};

const recorded = () => window.__activation?.records ?? [];

/** Surfaces this activation opened and then closed again — the dead-control
 *  signature. Only changes recorded while the activating event was still
 *  running count; a surface that closes later did so on its own schedule. */
function selfClosedSurfaces(records) {
  const openedIn = new Map();
  const dead = [];
  for (const record of records) {
    if (record.attribute !== "hidden" || record.ignored) continue;
    if (record.wasHidden && !record.isHidden) openedIn.set(record.name, record.batch);
    else if (!record.wasHidden && record.isHidden && openedIn.has(record.name)) {
      dead.push(`${record.name} (opened and closed again in the same activation)`);
    }
  }
  return dead;
}

/** Runs one activation and reports what it did. `synchronous` holds only what
 *  happened while the activating event itself was still dispatching. */
async function activate(page, label, click) {
  await page.evaluate(startRecording);
  await click();
  const synchronous = await page.evaluate(recorded);
  let effects = [];
  await expect
    .poll(
      async () => {
        effects = (await page.evaluate(recorded)).filter((record) => !record.ignored);
        return effects.length;
      },
      {
        timeout: 3_000,
        message: `${label} changed nothing a user could perceive — an inert control`,
      },
    )
    .toBeGreaterThan(0);
  return { dead: selfClosedSurfaces(synchronous), effects };
}

async function loadEditor(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
}

test.describe("every menu-bar command acts or explains itself", () => {
  for (const menu of MENUS) {
    test(`the ${menu} menu has no dead or inert row`, async ({ page }) => {
      test.setTimeout(240_000);
      const thrown = [];
      page.on("pageerror", (error) => thrown.push(String(error)));

      await loadEditor(page);
      await openAppMenu(page, menu);
      const rows = await page.$$eval("#appMenuPopover .app-menu-item", (items) =>
        items.map((item) => ({
          id: item.dataset.command,
          disabled: item.disabled,
          reason: item.title.trim(),
        })),
      );
      await page.keyboard.press("Escape");
      expect(rows.length, `the ${menu} menu offers rows`).toBeGreaterThan(0);

      for (const row of rows) {
        // A command that cannot run now must SAY SO. "Never a dead control"
        // covers the greyed case too: greyed with no reason is a control that
        // refuses in silence.
        if (row.disabled) {
          expect(row.reason, `${row.id} is greyed out and must give a reason`).not.toBe("");
          continue;
        }
        if (POINTER_UNSAFE.has(row.id)) continue;

        // A fresh document per row: this asks what ONE activation does, and the
        // row before it may have rewritten the document it acts on.
        await page.reload();
        await loadEditor(page);
        await openAppMenu(page, menu);
        thrown.length = 0;

        const { dead } = await activate(page, `${menu} ▸ ${row.id}`, () =>
          page.locator(`#appMenuPopover .app-menu-item[data-command="${row.id}"]`).click(),
        );
        expect(dead, `${menu} ▸ ${row.id} is a dead control`).toEqual([]);
        expect(thrown, `${menu} ▸ ${row.id} threw`).toEqual([]);
      }
    });
  }

  test("every skipped command still exists in the bar", async ({ page }) => {
    await loadEditor(page);
    const offered = new Set();
    for (const menu of MENUS) {
      await openAppMenu(page, menu);
      for (const id of await page.$$eval("#appMenuPopover .app-menu-item", (items) =>
        items.map((item) => item.dataset.command),
      )) {
        offered.add(id);
      }
      await page.keyboard.press("Escape");
    }
    // A skip that outlives its command is a hole in the sweep that nothing
    // would otherwise report.
    expect([...POINTER_UNSAFE.keys()].filter((id) => !offered.has(id))).toEqual([]);
  });
});

test.describe("every ribbon control acts or explains itself", () => {
  for (const tab of RIBBON_TABS) {
    test(`the ${tab} ribbon tab has no dead or inert button`, async ({ page }) => {
      test.setTimeout(300_000);
      const thrown = [];
      page.on("pageerror", (error) => thrown.push(String(error)));

      const panel = `#panel${tab[0].toUpperCase()}${tab.slice(1)}`;
      await loadEditor(page);
      await page.locator(`.ribbon-tab[data-tab="${tab}"]`).click();
      const buttons = await page.$$eval(`${panel} button`, (list) =>
        list
          // Only what a user can actually reach: the ribbon carries popovers
          // (colour grids, the table size picker) whose buttons live in the
          // panel but are not on screen until their own control is opened.
          .filter((button) => button.offsetParent !== null)
          .map((button, index) => ({
            index,
            id: button.dataset.command ?? button.dataset.commandId ?? button.id ?? "",
            disabled: button.disabled,
          })),
      );
      expect(buttons.length, `the ${tab} tab has buttons`).toBeGreaterThan(0);

      for (const button of buttons) {
        if (button.disabled || POINTER_UNSAFE.has(button.id)) continue;

        await page.reload();
        await loadEditor(page);
        await page.locator(`.ribbon-tab[data-tab="${tab}"]`).click();
        thrown.length = 0;

        const { dead } = await activate(page, `ribbon ${tab} ▸ ${button.id}`, () =>
          page.locator(`${panel} button`).nth(button.index).click(),
        );
        expect(dead, `ribbon ${tab} ▸ ${button.id} is a dead control`).toEqual([]);
        expect(thrown, `ribbon ${tab} ▸ ${button.id} threw`).toEqual([]);
      }
    });
  }
});

test.describe("Settings is reachable from every surface that offers it", () => {
  // The reported instance, asked of each pointer surface in turn. The menu and
  // the palette both activate on click, so both were dead; the gear was not,
  // which is exactly why the defect read as "the menu is broken" rather than
  // "settings is broken".
  const panel = "#settingsPanel";

  test("from the Tools menu", async ({ page, consoleErrors }) => {
    await loadEditor(page);
    await openAppMenu(page, "tools");
    await page.locator('#appMenuPopover .app-menu-item[data-command="view.settings"]').click();
    await expect(page.locator(panel)).toBeVisible();
    // Open means usable, not merely present: the panel takes the keyboard.
    await expect(page.locator(`${panel} :focus`)).toHaveCount(1);
    expect(consoleErrors).toEqual([]);
  });

  test("from the command palette", async ({ page, consoleErrors }) => {
    await loadEditor(page);
    await openAppMenu(page, "help");
    await page.locator('#appMenuPopover .app-menu-item[data-command="help.commands"]').click();
    await page.locator("#cmdInput").fill("settings");
    await page.locator("#cmdList .cmd-item").first().click();
    await expect(page.locator(panel)).toBeVisible();
    // The palette closes as it runs the command and hands focus back to its
    // opener; the panel must still end up with the keyboard, or "open" means
    // only that something was painted.
    await expect(page.locator(`${panel} :focus`)).toHaveCount(1);
    expect(consoleErrors).toEqual([]);
  });

  test("from the gear button, which still toggles", async ({ page, consoleErrors }) => {
    await loadEditor(page);
    await page.locator("#settingsBtn").click();
    await expect(page.locator(panel)).toBeVisible();
    await page.locator("#settingsBtn").click();
    await expect(page.locator(panel)).toBeHidden();
    expect(consoleErrors).toEqual([]);
  });

  test("and a click on the document still dismisses it", async ({ page, consoleErrors }) => {
    // The fix moved light dismiss from `click` to `pointerdown`; the behaviour
    // it protects must survive that, or this would trade one defect for another.
    await loadEditor(page);
    await openAppMenu(page, "tools");
    await page.locator('#appMenuPopover .app-menu-item[data-command="view.settings"]').click();
    await expect(page.locator(panel)).toBeVisible();
    await page.locator(".page-wrap .page").first().click({ position: { x: 60, y: 60 } });
    await expect(page.locator(panel)).toBeHidden();
    expect(consoleErrors).toEqual([]);
  });
});
