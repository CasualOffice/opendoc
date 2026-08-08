import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

// Review sits after Format, mirroring where Word puts its Review tab.
const MENU_LABELS = ["File", "Edit", "View", "Insert", "Format", "Review", "Tools", "Help"];

test("the Vellum-style title block exposes real menus and honest local document state", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  await expect(page.locator("#documentChrome")).toBeVisible();
  await expect(page.locator("#documentStateText")).toHaveText("Opened");
  await expect(page.locator(".app-menu-button")).toHaveText(MENU_LABELS);

  const verticalOrder = await page.evaluate(() => {
    const title = document.querySelector(".document-title-row").getBoundingClientRect();
    const menus = document.querySelector("#appMenuBar").getBoundingClientRect();
    return { titleBottom: title.bottom, menuTop: menus.top };
  });
  expect(verticalOrder.menuTop).toBeGreaterThanOrEqual(verticalOrder.titleBottom - 1);

  await clickIntoFirstPage(page);
  await page.keyboard.type("M");
  await expect(page.locator("#documentStateText")).toHaveText("Edited");

  await page.locator('.app-menu-button[data-menu="file"]').click();
  const menu = page.locator("#appMenuPopover");
  await expect(menu).toBeVisible();
  await expect(menu.locator('[data-command="file.open"]')).toContainText("Open");
  await expect(menu.locator('[data-command="file.save"]')).toContainText("Save");
  await expect(menu.locator('[data-command="file.properties"]')).toContainText("Document properties");

  const download = page.waitForEvent("download");
  await menu.locator('[data-command="file.save"]').click();
  await download;
  await expect(page.locator("#documentStateText")).toHaveText("Downloaded");

  await page.locator("#undoBtn").click();
  await expect(page.locator("#documentStateText")).toHaveText("Edited");
  expect(consoleErrors).toEqual([]);
});

test("application menus support keyboard traversal, disabled reasons, and real destinations", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const edit = page.locator('.app-menu-button[data-menu="edit"]');
  await edit.focus();
  await page.keyboard.press("ArrowDown");
  const menu = page.locator("#appMenuPopover");
  await expect(menu).toBeVisible();
  await expect(menu.locator('[data-command="edit.copy"]')).toBeDisabled();
  await expect(menu.locator('[data-command="edit.copy"]')).toHaveAttribute(
    "title",
    "Select content to copy",
  );

  // Left/right moves between top-level categories while the menu stays open.
  await page.keyboard.press("ArrowRight");
  await expect(page.locator('.app-menu-button[data-menu="view"]')).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  await expect(menu.locator('[data-command="view.outline"]')).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator('.app-menu-button[data-menu="insert"]')).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  await expect(menu.locator('[data-command="insert.table"]')).toBeFocused();

  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await expect(page.locator('.app-menu-button[data-menu="insert"]')).toBeFocused();

  await page.locator('.app-menu-button[data-menu="file"]').click();
  await menu.locator('[data-command="file.properties"]').click();
  await expect(page.locator("#propertiesPanel")).toBeVisible();
  await page.locator("#propertiesClose").click();

  await page.locator('.app-menu-button[data-menu="help"]').click();
  await menu.locator('[data-command="help.commands"]').click();
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator('.app-menu-button[data-menu="help"]')).toBeFocused();
  expect(consoleErrors).toEqual([]);
});

test("the two-row header contains its width and keeps every menu reachable on a narrow viewport", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 480, height: 720 });
  await gotoEditor(page);

  const containment = await page.evaluate(() => ({
    viewport: document.documentElement.clientWidth,
    documentWidth: document.documentElement.scrollWidth,
    headerRight: document.querySelector(".bar").getBoundingClientRect().right,
    menuScrollable:
      document.querySelector("#appMenuBar").scrollWidth >=
      document.querySelector("#appMenuBar").clientWidth,
  }));
  expect(containment.documentWidth).toBeLessThanOrEqual(containment.viewport);
  expect(containment.headerRight).toBeLessThanOrEqual(containment.viewport);
  expect(containment.menuScrollable).toBe(true);

  const help = page.locator('.app-menu-button[data-menu="help"]');
  await help.evaluate((button) => button.scrollIntoView({ inline: "nearest", block: "nearest" }));
  await help.click();
  const popoverBox = await page.locator("#appMenuPopover").boundingBox();
  expect(popoverBox.x).toBeGreaterThanOrEqual(0);
  expect(popoverBox.x + popoverBox.width).toBeLessThanOrEqual(480);
  await expect(page.locator('[data-command="help.commands"]')).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("document dialogs share the standard type and sizing system", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  await page.locator("#propertiesBtn").click();
  const propertiesMetrics = await page.locator(".properties-dialog").evaluate((card) => {
    const number = (value) => Number.parseFloat(value);
    const style = getComputedStyle(card);
    const title = card.querySelector("h2");
    const close = card.querySelector(".dialog-close");
    const input = card.querySelector(".dialog-field input");
    const action = card.querySelector(".dialog-button");
    return {
      width: card.getBoundingClientRect().width,
      radius: number(style.borderRadius),
      titleSize: number(getComputedStyle(title).fontSize),
      closeSize: close.getBoundingClientRect().height,
      fieldHeight: input.getBoundingClientRect().height,
      actionHeight: action.getBoundingClientRect().height,
      fontFamily: getComputedStyle(document.body).fontFamily,
    };
  });
  expect(propertiesMetrics.width).toBeCloseTo(640, 0);
  expect(propertiesMetrics.radius).toBe(12);
  expect(propertiesMetrics.titleSize).toBe(16);
  expect(propertiesMetrics.closeSize).toBe(30);
  expect(propertiesMetrics.fieldHeight).toBe(36);
  expect(propertiesMetrics.actionHeight).toBe(34);
  expect(propertiesMetrics.fontFamily).toContain("Inter");
  await page.locator("#propertiesClose").click();

  await page.locator('.app-menu-button[data-menu="tools"]').click();
  await page.locator('#appMenuPopover [data-command="layout.pageSetup"]').click();
  await expect(page.locator("#pageSetupMenu")).toBeVisible();
  const pageSetupMetrics = await page.locator(".page-setup-dialog").evaluate((card) => ({
    width: card.getBoundingClientRect().width,
    radius: Number.parseFloat(getComputedStyle(card).borderRadius),
    titleSize: Number.parseFloat(getComputedStyle(card.querySelector("h2")).fontSize),
    closeSize: card.querySelector(".dialog-close").getBoundingClientRect().height,
    fieldHeight: card.querySelector(".dialog-select").getBoundingClientRect().height,
    actionHeight: card.querySelector(".dialog-button").getBoundingClientRect().height,
  }));
  expect(pageSetupMetrics).toEqual({
    width: 760,
    radius: 12,
    titleSize: 16,
    closeSize: 30,
    fieldHeight: 36,
    actionHeight: 34,
  });
  await page.locator("#pageSetupClose").click();

  await expect(page.locator("#splitCellDialog .dialog-card")).toHaveClass(
    /dialog-card-compact/,
  );
  await expect(page.locator("#splitCellDialog .dialog-card")).not.toHaveAttribute("style", /.+/);
  expect(consoleErrors).toEqual([]);
});
