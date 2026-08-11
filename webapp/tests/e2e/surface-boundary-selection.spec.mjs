// A drag belongs to the editing surface where pointer-down occurred.
//
// Header/footer bodies and text-box bodies are separate WordprocessingML
// stories. A range spanning either one and the document body is invalid: copy,
// delete, formatting, and replacement cannot give it deterministic semantics.
// Doc 58 therefore clips a drag at the starting surface instead of silently
// switching context midway through the gesture.
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

async function selectObject(page, box) {
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

test("a header-to-body drag stays in the header story", async ({
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

  // Begin on the header's real line, then cross well into the body. The moving
  // end may clip to the header edge; the active story must not change.
  await page.mouse.move(box.x + 80, box.y + 12);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.45, {
    steps: 12,
  });
  await page.mouse.up();

  await expect(page.locator("#pages")).toHaveAttribute(
    "data-running-edit",
    "header",
  );
  await page.keyboard.type("AFTERBOUNDARY");
  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Typing",
  );
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

  expect(consoleErrors).toEqual([]);
});

test("a text-box-to-body drag stays in the text-box story", async ({
  page,
  consoleErrors,
}) => {
  const { box } = await openFile(page, TEXTBOX);
  const bodyBefore = await page.locator("#a11yDocument").textContent();

  await selectObject(page, box);
  const outline = page.locator(".overlay .object-outline");
  await expect(outline).toBeVisible();
  const rect = await outline.boundingBox();
  expect(rect).not.toBeNull();

  const start = {
    x: rect.x + rect.width * 0.15,
    y: rect.y + rect.height * 0.5,
  };
  await page.mouse.dblclick(start.x, start.y);
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );

  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.55, {
    steps: 12,
  });
  await page.mouse.up();

  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );
  await page.keyboard.type("AFTERBOUNDARY");
  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Typing",
  );
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

  expect(consoleErrors).toEqual([]);
});
