import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
} from "./fixtures.mjs";

async function selectTypedMarker(page, marker) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(marker);
  for (let index = 0; index < marker.length; index++) {
    await page.keyboard.press("Shift+ArrowLeft");
  }
}

async function insertTwoByTwoTable(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
}

test("right-click preserves a text selection and exposes context-aware commands", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectTypedMarker(page, "CONTEXT_SELECTION");
  const highlightsBefore = await page.locator(".overlay .highlight").count();
  expect(highlightsBefore).toBeGreaterThan(0);
  const box = await page.locator(".overlay .highlight").first().boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2, {
    button: "right",
  });

  const menu = page.locator(".editor-context-menu");
  await expect(menu).toBeVisible();
  await expect(menu.locator('[data-command-id="edit.copy"]')).toBeEnabled();
  await expect(menu.locator('[data-command-id="edit.cut"]')).toBeEnabled();
  await expect(menu.locator('[data-command-id="comment.add"]')).toBeEnabled();
  await expect(menu.locator('[data-command-id="link.add"]')).toBeEnabled();
  await expect(menu.locator('[data-command-id="paragraph.properties"]')).toBeVisible();
  await expect(page.locator(".overlay .highlight")).toHaveCount(highlightsBefore);

  const bounds = await menu.boundingBox();
  const viewport = page.viewportSize();
  expect(bounds.x).toBeGreaterThanOrEqual(8);
  expect(bounds.y).toBeGreaterThanOrEqual(8);
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(viewport.width - 8);
  expect(bounds.y + bounds.height).toBeLessThanOrEqual(viewport.height - 8);

  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await expect(page.locator("#pages")).toBeFocused();
  await expect(page.locator(".overlay .highlight")).toHaveCount(highlightsBefore);
  expect(consoleErrors).toEqual([]);
});

test("Shift+F10 supports menu keyboard navigation and theme-aware surfaces", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectTypedMarker(page, "KEYBOARD_CONTEXT");
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "dark";
  });
  await page.keyboard.press("Shift+F10");

  const menu = page.locator(".editor-context-menu");
  await expect(menu).toBeVisible();
  const surface = await menu.evaluate((element) => {
    const style = getComputedStyle(element);
    const probe = document.createElement("div");
    probe.style.backgroundColor = "var(--surface)";
    document.body.appendChild(probe);
    const expected = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return {
      background: style.backgroundColor,
      expected,
      color: style.color,
      radius: style.borderRadius,
    };
  });
  expect(surface.background).toBe(surface.expected);
  expect(surface.color).not.toBe("rgb(0, 0, 0)");
  expect(surface.radius).not.toBe("0px");

  const focusedBefore = await page.evaluate(() =>
    document.activeElement?.dataset.commandId);
  await page.keyboard.press("ArrowDown");
  const focusedAfter = await page.evaluate(() =>
    document.activeElement?.dataset.commandId);
  expect(focusedAfter).not.toBe(focusedBefore);
  await page.keyboard.press("End");
  await expect(menu.locator(".menu-item.active")).toHaveAttribute(
    "data-command-id",
    "paragraph.properties",
  );
  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).toBeFocused();
  expect(consoleErrors).toEqual([]);
});

test("link and comment ranges receive their exact contextual actions", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectTypedMarker(page, "LINK_COMMENT_CONTEXT");
  await page.locator('[data-tab="insert"]').click();
  page.once("dialog", (dialog) => dialog.accept("https://example.com/context"));
  await page.locator("#insertLinkBtn").click();

  let box = await page.locator(".overlay .highlight").first().boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2, {
    button: "right",
  });
  const menu = page.locator(".editor-context-menu");
  await expect(menu.locator('[data-command-id="link.edit"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="link.remove"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="link.add"]')).toHaveCount(0);
  await menu.locator('[data-command-id="link.remove"]').click();
  await expect(menu).toBeHidden();

  await page.locator("#selComment").click();
  const composer = page.locator('[data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.fill("Context menu comment");
  await page.locator('[data-testid="review-comment-submit"]').click();
  const marker = page.locator(".review-comment-marker").first();
  await expect(marker).toBeVisible();
  box = await marker.boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2, {
    button: "right",
  });
  await expect(menu.locator('[data-command-id="comment.open"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="comment.add"]')).toHaveCount(0);
  await menu.locator('[data-command-id="comment.open"]').click();
  await expect(
    page.locator(".review-margin-card.review-margin-comment"),
  ).toHaveAttribute("aria-expanded", "true");
  expect(consoleErrors).toEqual([]);
});

test("table and suggestion contexts expose exact commands and mode-safe reasons", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwoTable(page);
  await page.locator("#pages").focus();
  await page.keyboard.press("Shift+F10");
  const menu = page.locator(".editor-context-menu");
  const submenu = page.locator(".editor-submenu");
  await expect(menu).toBeVisible();
  // Structure ops live in Insert / Delete / Select submenus, not a flat dump.
  await expect(menu.locator('[data-command-id="table.insert"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="table.delete"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="table.properties"]')).toBeVisible();
  await menu.locator('[data-command-id="table.insert"]').click();
  await expect(submenu.locator('[data-command-id="table.insert.rowAbove"]')).toBeEnabled();
  await expect(submenu.locator('[data-command-id="table.insert.columnRight"]')).toBeEnabled();
  await menu.locator('[data-command-id="table.delete"]').click();
  await expect(submenu.locator('[data-command-id="table.delete.table"]')).toBeEnabled();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");

  await page.locator("#pages").focus();
  await page.keyboard.press("Shift+F10");
  await menu.locator('[data-command-id="table.select"]').click();
  await submenu.locator('[data-command-id="table.select.row"]').click();
  await expect(page.locator(".table-cell-selection")).toHaveCount(2);
  await page.locator("#pages").focus();
  await page.keyboard.press("Shift+F10");
  await expect(menu.locator('[data-command-id="table.merge"]')).toBeEnabled();
  await page.keyboard.press("Escape");

  await setReviewMode(page, "suggesting");
  await page.locator("#pages").focus();
  await page.keyboard.press("Shift+F10");
  await menu.locator('[data-command-id="table.insert"]').click();
  await expect(
    submenu.locator('[data-command-id="table.insert.rowAbove"]'),
  ).toBeDisabled();
  await expect(
    submenu.locator('[data-command-id="table.insert.rowAbove"] .menu-item-hint'),
  ).toContainText("cannot be tracked");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");

  await page.locator("#suggestingBanner").getByRole("button", {
    name: "Switch to editing",
  }).click();
  await moveCaretToDocStart(page);
  await setReviewMode(page, "suggesting");
  await page.keyboard.type("REVIEW_CONTEXT");
  await page.keyboard.press("Shift+ArrowLeft");
  await page.keyboard.press("Shift+F10");
  await expect(menu.locator('[data-command-id="review.accept"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="review.reject"]')).toBeVisible();
  await menu.locator('[data-command-id="review.accept"]').click();
  await expect(page.locator(".review-margin-card.review-margin-insertion")).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("the menu is contextual — a table cell exposes table tools a text selection does not", async ({
  page,
  consoleErrors,
}) => {
  // Prose selection: text tools present, table submenus absent.
  await gotoEditor(page);
  await selectTypedMarker(page, "CONTEXTUAL_MENU");
  const box = await page.locator(".overlay .highlight").first().boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2, {
    button: "right",
  });
  const menu = page.locator(".editor-context-menu");
  await expect(menu).toBeVisible();
  await expect(menu.locator('[data-command-id="format.menu"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="table.insert"]')).toHaveCount(0);
  await expect(menu.locator('[data-command-id="table.delete"]')).toHaveCount(0);
  await page.keyboard.press("Escape");

  // Table cell: the very same surface now gains Insert / Delete / Select tools.
  await insertTwoByTwoTable(page);
  await page.locator("#pages").focus();
  await page.keyboard.press("Shift+F10");
  await expect(menu.locator('[data-command-id="table.insert"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="table.delete"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="table.select"]')).toBeVisible();
  expect(consoleErrors).toEqual([]);
});
