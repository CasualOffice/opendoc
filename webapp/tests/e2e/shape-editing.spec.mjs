// Editing what a DRAWING LOOKS LIKE — Word's Shape Format tab.
//
// A shape rendered, selected, moved and resized, and there was no way to change
// its fill or its outline: the model carried both, the op set carried neither.
// The object bar also called every non-text-box object an "Image" and handed a
// shape a Crop button it has no source rectangle for.
//
// No fixture had a shape, which is why none of that was visible. `shapes.docx`
// is built for these tests: a lone autoshape (which imports as a group-of-one,
// exactly as Word's Insert > Shapes does) and an ellipse inside a real group
// beside a text box.
import { test, expect, gotoEditor, stableBox } from "./fixtures.mjs";

const SHAPES = "../fixtures/generated/shapes.docx";

async function open(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(SHAPES);
  const canvas = page.locator(".page-wrap .page").first();
  await expect(canvas).toBeVisible();
  const box = await stableBox(canvas);
  await expect(page.locator("#a11yDocument")).toContainText("Body before the shapes");
  return box;
}

/** Clicks around until a shape is selected, so the point is found rather than
 *  assumed. Returns the point that worked. */
async function selectShape(page, box) {
  for (let fy = 0.04; fy < 0.5; fy += 0.02) {
    for (let fx = 0.08; fx < 0.85; fx += 0.04) {
      const p = { x: box.x + box.width * fx, y: box.y + box.height * fy };
      // A true multi-child group is selected as a unit on first click;
      // double-click explicitly descends to the painted child.
      await page.mouse.dblclick(p.x, p.y);
      if ((await page.locator("#pages").getAttribute("data-object-kind")) === "shape") return p;
    }
  }
  throw new Error("no shape could be selected");
}

/** The selected shape's fill/outline as the editor reflects it — the same values
 *  the Fill/Outline swatches paint from, read back through `#pages` like every
 *  other piece of selection state. `"none"` means the shape has none. */
async function shapeFormat(page) {
  return page.evaluate(() => {
    const d = document.getElementById("pages").dataset;
    if (d.shapeFill === undefined) return null;
    const norm = (v) => (v === "none" ? null : v);
    return {
      fill: norm(d.shapeFill),
      outline: norm(d.shapeOutline),
      width: d.shapeOutlineWidth === "none" ? null : Number(d.shapeOutlineWidth),
    };
  });
}

test("a shape is selectable and named a Shape, not an Image", async ({ page, consoleErrors }) => {
  const box = await open(page);
  await selectShape(page, box);

  // It used to read "Image" — and got an Image's Crop button with it.
  const bar = page.locator(".object-context-bar");
  await expect(bar).toBeVisible();
  await expect(bar.locator("strong")).toHaveText("Shape");
  await expect(bar.getByRole("button", { name: "Crop image" })).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("Shape Fill applies a color and undo puts the old one back", async ({
  page,
  consoleErrors,
}) => {
  const box = await open(page);
  await selectShape(page, box);
  const before = await shapeFormat(page);
  expect(before.fill, "the fixture's shapes are filled").toBeTruthy();

  await page.locator(".object-context-bar").getByRole("button", { name: "Shape fill" }).click();
  const menu = page.locator("#shapeFillMenu");
  await expect(menu).toBeVisible();
  await menu.locator('[data-shape-color="#ff0000"]').click();

  await expect.poll(async () => (await shapeFormat(page))?.fill).toBe("#ff0000");
  await expect(page.locator("#undoBtn")).toBeEnabled();

  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await shapeFormat(page))?.fill).toBe(before.fill);

  expect(consoleErrors).toEqual([]);
});

test("No fill clears the fill entirely", async ({ page, consoleErrors }) => {
  const box = await open(page);
  await selectShape(page, box);

  await page.locator(".object-context-bar").getByRole("button", { name: "Shape fill" }).click();
  await page.locator("#shapeFillMenu [data-shape-none]").click();

  await expect.poll(async () => (await shapeFormat(page))?.fill).toBeNull();
  expect(consoleErrors).toEqual([]);
});

test("Shape Outline sets a color and a weight independently", async ({ page, consoleErrors }) => {
  const box = await open(page);
  await selectShape(page, box);

  const bar = page.locator(".object-context-bar");
  await bar.getByRole("button", { name: "Shape outline" }).click();
  await page.locator('#shapeOutlineMenu [data-shape-color="#00ff00"]').click();
  await expect.poll(async () => (await shapeFormat(page))?.outline).toBe("#00ff00");

  // Changing the weight keeps the color: they are separate controls in Word and
  // must not overwrite one another.
  await bar.getByRole("button", { name: "Shape outline" }).click();
  await page.locator('#shapeOutlineMenu [data-shape-weight="38100"]').click(); // 3 pt
  const after = await shapeFormat(page);
  expect(after.width).toBe(38100);
  expect(after.outline).toBe("#00ff00");

  expect(consoleErrors).toEqual([]);
});

test("No outline removes the outline", async ({ page, consoleErrors }) => {
  const box = await open(page);
  await selectShape(page, box);

  await page.locator(".object-context-bar").getByRole("button", { name: "Shape outline" }).click();
  await page.locator("#shapeOutlineMenu [data-shape-none]").click();

  await expect.poll(async () => (await shapeFormat(page))?.outline).toBeNull();
  expect(consoleErrors).toEqual([]);
});

test("the fill actually repaints the canvas", async ({ page, consoleErrors }) => {
  // The whole point is what the user SEES. Assert on pixels: the page must look
  // different after a fill, and an engine change that never reached the raster
  // would leave it identical.
  const box = await open(page);
  await selectShape(page, box);
  const canvas = page.locator(".page-wrap .page").first();
  const before = await canvas.screenshot();

  await page.locator(".object-context-bar").getByRole("button", { name: "Shape fill" }).click();
  await page.locator('#shapeFillMenu [data-shape-color="#ff0000"]').click();
  await expect.poll(async () => (await shapeFormat(page))?.fill).toBe("#ff0000");
  await page.waitForTimeout(300);

  const after = await canvas.screenshot();
  expect(Buffer.compare(before, after), "filling a shape must change the page").not.toBe(0);

  expect(consoleErrors).toEqual([]);
});

test("the picker reflects the shape it is on, not the last color used", async ({
  page,
  consoleErrors,
}) => {
  const box = await open(page);
  const first = await selectShape(page, box);
  await page.locator(".object-context-bar").getByRole("button", { name: "Shape fill" }).click();
  await page.locator('#shapeFillMenu [data-shape-color="#ff0000"]').click();
  await expect.poll(async () => (await shapeFormat(page))?.fill).toBe("#ff0000");
  const firstNode = await page.locator("#pages").getAttribute("data-object-selected");

  // Select the OTHER shape (the grouped ellipse) and open the picker again: the
  // active swatch must describe that shape, which was never touched.
  let secondNode = firstNode;
  for (let fy = 0.04; fy < 0.6 && secondNode === firstNode; fy += 0.02) {
    for (let fx = 0.08; fx < 0.85; fx += 0.04) {
      const p = { x: box.x + box.width * fx, y: box.y + box.height * fy };
      if (Math.abs(p.x - first.x) < 20 && Math.abs(p.y - first.y) < 20) continue;
      await page.mouse.dblclick(p.x, p.y);
      const kind = await page.locator("#pages").getAttribute("data-object-kind");
      const node = await page.locator("#pages").getAttribute("data-object-selected");
      if (kind === "shape" && node && node !== firstNode) {
        secondNode = node;
        break;
      }
    }
  }
  expect(secondNode, "the fixture has a second, grouped shape").not.toBe(firstNode);

  const second = await shapeFormat(page);
  expect(second.fill).not.toBe("#ff0000");
  await page.locator(".object-context-bar").getByRole("button", { name: "Shape fill" }).click();
  await expect(page.locator("#shapeFillMenu [data-shape-color].is-active")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("a shape inside a group can be filled", async ({ page, consoleErrors }) => {
  // Resolution that starts at the body — the seam that broke header typing,
  // selection and formatting in turn — would find a top-level shape and miss
  // this one.
  const box = await open(page);
  const seen = new Set();
  let filled = false;
  for (let fy = 0.04; fy < 0.6 && !filled; fy += 0.02) {
    for (let fx = 0.08; fx < 0.85; fx += 0.04) {
      await page.mouse.dblclick(box.x + box.width * fx, box.y + box.height * fy);
      if ((await page.locator("#pages").getAttribute("data-object-kind")) !== "shape") continue;
      const node = await page.locator("#pages").getAttribute("data-object-selected");
      if (seen.has(node)) continue;
      seen.add(node);
      if (seen.size < 2) continue; // the second distinct shape is the grouped one
      await page.locator(".object-context-bar").getByRole("button", { name: "Shape fill" }).click();
      await page.locator('#shapeFillMenu [data-shape-color="#4a86e8"]').click();
      await expect.poll(async () => (await shapeFormat(page))?.fill).toBe("#4a86e8");
      filled = true;
      break;
    }
  }
  expect(filled, "the grouped shape must be reachable and fillable").toBe(true);

  expect(consoleErrors).toEqual([]);
});

test("a multi-child group selects as a unit and Enter descends with a stable reference", async ({
  page,
  consoleErrors,
}) => {
  const box = await open(page);
  const pages = page.locator("#pages");
  let found = false;
  for (let fy = 0.04; fy < 0.6 && !found; fy += 0.02) {
    for (let fx = 0.08; fx < 0.85; fx += 0.04) {
      await page.mouse.click(box.x + box.width * fx, box.y + box.height * fy);
      found = (await pages.getAttribute("data-object-kind")) === "group";
      if (found) break;
    }
  }
  expect(found, "the real group is reachable as one initial selection").toBe(true);
  const root = await pages.getAttribute("data-object-root");
  expect(root).toMatch(/^[0-9a-f]{32}$/);
  await expect(pages).toHaveAttribute("data-object-subject", root);
  await expect(pages).toHaveAttribute("data-object-path", "");
  await expect(pages).toHaveAttribute(
    "data-object-capabilities",
    "canMove,canWrap,canDelete",
  );

  await page.keyboard.press("Enter");
  await expect(pages).toHaveAttribute("data-object-kind", "shape");
  await expect(pages).toHaveAttribute("data-object-root", root);
  await expect(pages).not.toHaveAttribute("data-object-subject", root);
  await expect(pages).toHaveAttribute("data-object-path", /\d+(\.\d+)*/);
  await expect(
    page.locator(".object-context-bar").getByRole("button", { name: "Shape fill" }),
  ).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

test("Shape Fill is reachable from the right-click menu, not only the bar", async ({
  page,
  consoleErrors,
}) => {
  // The recurring defect in this editor is a capability wired into exactly one
  // surface. Fill and Outline are on the object bar; they must also be on the
  // object menu, and the menu entry must run the same op.
  const box = await open(page);
  const p = await selectShape(page, box);
  await page.mouse.click(p.x, p.y, { button: "right" });

  const menu = page.locator(".editor-context-menu");
  await expect(menu).toBeVisible();
  await expect(menu.locator('[data-command-id="object.fill"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="object.outline"]')).toBeVisible();
  // A shape has no source rectangle, so the picture-only Crop entry must be gone.
  await expect(menu.locator('[data-command-id="object.crop"]')).toHaveCount(0);

  // A submenu renders in its own `.editor-submenu`, not inside the parent menu.
  await menu.locator('[data-command-id="object.fill"]').hover();
  const submenu = page.locator(".editor-submenu").last();
  await submenu.locator('[data-command-id="object.fill.#ff0000"]').click();
  await expect.poll(async () => (await shapeFormat(page))?.fill).toBe("#ff0000");

  expect(consoleErrors).toEqual([]);
});
