// Word's Insert ▸ Shapes and Insert ▸ Text Box.
//
// The editor could insert a picture, a table, a field, a symbol, an emoji, a
// footnote and a header — but not a text box and not a shape. It could SELECT,
// move, resize, edit and delete both; it just had no way to create one. A
// document that did not already contain a drawing could never gain one.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

async function open(page) {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
}

async function openInsertTab(page) {
  await page.locator("#tabInsert").click();
  await expect(page.locator("#panelInsert")).toBeVisible();
}

test("Insert ▸ Text box creates a box and puts the caret inside it", async ({
  page,
  consoleErrors,
}) => {
  await open(page);
  const bodyBefore = await page.locator("#a11yDocument").textContent();

  await openInsertTab(page);
  await page.locator("#insertTextBoxBtn").click();
  await expect(page.locator("#status")).toContainText("Text box added");

  // Word leaves you typing in the new box. Anything less means finding it first.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "textbox");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
  await page.keyboard.type("IN THE BOX");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  // The typing went into the box, not the page body.
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

  expect(consoleErrors).toEqual([]);
});

test("Insert ▸ Shapes offers every preset and inserts the chosen one, selected", async ({
  page,
  consoleErrors,
}) => {
  await open(page);
  await openInsertTab(page);
  await page.locator("#insertShapeBtn").click();

  const gallery = page.locator("#shapeGalleryMenu");
  await expect(gallery).toBeVisible();
  // Every entry must be a preset the renderer can actually draw; a gallery that
  // inserts an invisible shape is worse than a short one.
  await expect(gallery.locator("[data-shape-geometry]")).toHaveCount(7);

  await gallery.locator('[data-shape-geometry="ellipse"]').click();
  await expect(page.locator("#status")).toContainText("Ellipse added");

  // Word leaves a new shape selected — which is also what puts Fill and Outline
  // within reach without hunting for the thing you just made.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "shape");
  const bar = page.locator(".object-context-bar");
  await expect(bar.locator("strong")).toHaveText("Shape");
  await expect(bar.getByRole("button", { name: "Shape fill" })).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

test("an inserted group-child shape routes root commands and subject formatting", async ({
  page,
  consoleErrors,
}) => {
  await open(page);
  await openInsertTab(page);
  await page.locator("#insertShapeBtn").click();
  await page.locator('#shapeGalleryMenu [data-shape-geometry="rectangle"]').click();

  const pages = page.locator("#pages");
  await expect(pages).toHaveAttribute("data-object-kind", "shape");
  await expect(pages).toHaveAttribute("data-object-surface", "body");
  await expect(pages).toHaveAttribute("data-object-path", "0");
  const root = await pages.getAttribute("data-object-root");
  const subject = await pages.getAttribute("data-object-subject");
  expect(root).toMatch(/^[0-9a-f]{32}$/);
  expect(subject).toMatch(/^[0-9a-f]{32}$/);
  expect(root).not.toBe(subject);
  await expect(pages).toHaveAttribute(
    "data-object-capabilities",
    "canResize,canMove,canWrap,canDelete,canFill,canStroke",
  );

  // The subject owns Fill/Stroke while the stable reference's root owns resize,
  // move, wrap, and delete. Every group handle commits root extent + transform +
  // anchor through one exact-inverse engine transaction.
  await expect(page.locator(".overlay .object-handle")).toHaveCount(8);
  const bar = page.locator(".object-context-bar");
  await expect(bar).toContainText("handles to resize");
  await expect(bar.getByRole("button", { name: "Shape fill" })).toBeVisible();
  await expect(bar.getByRole("button", { name: "Shape outline" })).toBeVisible();
  await expect(bar.getByRole("button", { name: "Edit alt text" })).toHaveCount(0);
  await expect(bar.getByRole("button", { name: "Delete object" })).toBeVisible();
  await expect(page.locator(".object-wrap-btn")).toHaveCount(6);

  const beforeResize = await page
    .locator(".overlay .object-outline")
    .first()
    .evaluate((element) => {
      const rect = element.getBoundingClientRect();
      return { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
    });
  const nw = page.locator('.overlay .object-handle[data-handle="0"]').first();
  const nwBox = await nw.boundingBox();
  await page.mouse.move(nwBox.x + nwBox.width / 2, nwBox.y + nwBox.height / 2);
  await page.mouse.down();
  await page.mouse.move(nwBox.x - 50, nwBox.y - 30, { steps: 6 });
  await page.mouse.up();
  const resized = await page
    .locator(".overlay .object-outline")
    .first()
    .evaluate((element) => {
      const rect = element.getBoundingClientRect();
      return { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
    });
  expect(resized.left).toBeLessThan(beforeResize.left - 10);
  expect(resized.top).toBeLessThan(beforeResize.top - 10);
  expect(Math.abs(resized.right - beforeResize.right)).toBeLessThanOrEqual(3);
  expect(Math.abs(resized.bottom - beforeResize.bottom)).toBeLessThanOrEqual(3);
  await page.keyboard.press(`${MOD}+z`);
  await expect
    .poll(() =>
      page
        .locator(".overlay .object-outline")
        .first()
        .evaluate((element) => Math.round(element.getBoundingClientRect().left)),
    )
    .toBe(Math.round(beforeResize.left));

  // Keyboard geometry reads the subject rectangle and commits SetAnchor to the
  // root. This used to invoke a command against the incompatible child id.
  const beforeX = await page
    .locator(".overlay .object-outline")
    .first()
    .evaluate((element) => element.getBoundingClientRect().left);
  await page.keyboard.press("ArrowRight");
  await expect
    .poll(() =>
      page
        .locator(".overlay .object-outline")
        .first()
        .evaluate((element) => element.getBoundingClientRect().left),
    )
    .toBeGreaterThan(beforeX);

  const behind = bar.getByRole("button", { name: "Behind text" });
  await behind.click();
  await expect(behind).toHaveAttribute("aria-pressed", "true");

  await bar.getByRole("button", { name: "Delete object" }).click();
  await expect(pages).not.toHaveAttribute("data-object-kind", "shape");

  expect(consoleErrors).toEqual([]);
});

test("an inserted shape is undoable in one step", async ({ page, consoleErrors }) => {
  await open(page);
  await openInsertTab(page);
  await page.locator("#insertShapeBtn").click();
  await page.locator('#shapeGalleryMenu [data-shape-geometry="rectangle"]').click();
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "shape");

  await page.keyboard.press(`${MOD}+z`);
  // One undo removes it completely — no half-inserted group left behind.
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-kind", "shape");

  expect(consoleErrors).toEqual([]);
});

test("both inserts are reachable from the command palette too", async ({
  page,
  consoleErrors,
}) => {
  // The recurring defect in this editor is a capability wired into exactly one
  // surface. `insert-surface.spec.mjs` pins the ribbon against the Insert menu;
  // this pins the palette.
  await open(page);
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Text box");
  await page.locator("#cmdList .cmd-item", { hasText: "Text box" }).first().click();
  await expect(page.locator("#status")).toContainText("Text box added");

  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Shape");
  await page.locator("#cmdList .cmd-item", { hasText: "Shape" }).first().click();
  await expect(page.locator("#shapeGalleryMenu")).toBeVisible();

  expect(consoleErrors).toEqual([]);
});
