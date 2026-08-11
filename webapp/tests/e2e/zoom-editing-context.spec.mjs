// Zoom is a geometry change, not an editing-context transition. Rebuilding the
// page DOM must preserve the model selection, its owning story, the visible
// context chrome, and the destination of the next keystroke (docs/58).
import { test, expect, gotoEditor, stableBox } from "./fixtures.mjs";

const TEXTBOX = "../fixtures/generated/inline-text-box.docx";

async function openFile(page, file) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(file);
  const canvas = page.locator(".page-wrap .page").first();
  await expect(canvas).toBeVisible();
  return { canvas, box: await stableBox(canvas) };
}

async function setZoom(page, value) {
  const zoom = page.locator("#zoom");
  const before = await stableBox(page.locator(".page-wrap .page").first());
  await zoom.fill(value);
  await zoom.press("Enter");
  await expect(zoom).toHaveValue(value);
  const after = await stableBox(page.locator(".page-wrap .page").first());
  expect(after.width).toBeGreaterThan(before.width * 1.4);
}

async function fitWidth(page) {
  await page.locator("#zoomMenuBtn").click();
  const menu = page.locator("#zoomMenu");
  await expect(menu).toBeVisible();
  await menu.locator('[data-zoom-mode="fit-width"]').click();
  await expect(page.locator("#zoom")).toHaveValue("Fit width");
  await expect(page.locator(".page-wrap .page").first()).toBeVisible();
}

async function selectTextBox(page, box) {
  for (let fy = 0.04; fy < 0.45; fy += 0.02) {
    for (let fx = 0.1; fx < 0.85; fx += 0.05) {
      await page.mouse.click(box.x + box.width * fx, box.y + box.height * fy);
      if (
        (await page.locator("#pages").getAttribute("data-object-kind")) ===
        "textbox"
      )
        return;
    }
  }
  throw new Error("no text box found");
}

test("zoom preserves header context, caret, chrome, and typing destination", async ({
  page,
  consoleErrors,
}) => {
  const { box } = await openFile(page, "sample.docx");
  const bodyBefore = await page.locator("#a11yDocument").textContent();

  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 12);
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-running-edit",
    "header",
  );
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  await expect(page.locator(".running-band").first()).toBeVisible();

  await fitWidth(page);

  await expect(page.locator("#pages")).toHaveAttribute(
    "data-running-edit",
    "header",
  );
  await expect(page.locator("body")).toHaveClass(/running-edit/);
  await expect(page.locator(".running-band").first()).toBeVisible();
  await expect(page.locator(".overlay .caret")).toBeVisible();

  await page.keyboard.type("AFTERZOOM");
  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Typing",
  );
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);
  expect(consoleErrors).toEqual([]);
});

test("zoom preserves text-box context, caret, and typing destination", async ({
  page,
  consoleErrors,
}) => {
  const { box } = await openFile(page, TEXTBOX);
  const bodyBefore = await page.locator("#a11yDocument").textContent();

  await selectTextBox(page, box);
  const outline = page.locator(".overlay .object-outline");
  await expect(outline).toBeVisible();
  const rect = await outline.boundingBox();
  expect(rect).not.toBeNull();
  await page.mouse.dblclick(
    rect.x + rect.width * 0.15,
    rect.y + rect.height * 0.5,
  );
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );
  await expect(page.locator(".overlay .caret")).toBeVisible();

  await setZoom(page, "150%");

  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );
  await expect(page.locator(".overlay .caret")).toBeVisible();
  await page.keyboard.type("AFTERZOOM");
  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Typing",
  );
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);
  expect(consoleErrors).toEqual([]);
});
