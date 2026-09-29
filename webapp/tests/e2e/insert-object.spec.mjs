// Word's Insert ▸ Shapes and Insert ▸ Text Box.
//
// The editor could insert a picture, a table, a field, a symbol, an emoji, a
// footnote and a header — but not a text box and not a shape. It could SELECT,
// move, resize, edit and delete both; it just had no way to create one. A
// document that did not already contain a drawing could never gain one.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  mirrorBlocks,
  expectTypedIntoOneBlock,
  stableBox,
  MOD,
} from "./fixtures.mjs";

async function open(page) {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
}


/** Clicks at a fraction of the first page. Used to finish a shape-draw gesture
 *  with a plain click, which is the "default size here" half of the contract. */
async function clickOnPage(page, fx, fy) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await page.mouse.click(box.x + box.width * fx, box.y + box.height * fy);
}

/** Drags a rectangle on the first page, in page fractions, which is how Word
 *  and every drawing tool place a shape at the size you want. */
async function dragOnPage(page, from, to) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  const at = (f) => ({ x: box.x + box.width * f.x, y: box.y + box.height * f.y });
  const start = at(from);
  const end = at(to);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(end.x, end.y, { steps: 8 });
  await page.mouse.up();
  return { box, start, end };
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
  const blocksBefore = await mirrorBlocks(page);

  await openInsertTab(page);
  await page.locator("#insertTextBoxBtn").click();
  await expect(page.locator("#status")).toContainText("Text box added");

  // Word leaves you typing in the new box. Anything less means finding it first.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "textbox");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
  await page.keyboard.type("IN THE BOX");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  // The typing went into the box, not the page body — and, since HF-169, the
  // box's own text is in the mirror, so that is asserted by naming the one
  // block it landed in rather than by the mirror never changing.
  expectTypedIntoOneBlock(
    blocksBefore,
    await mirrorBlocks(page),
    "IN THE BOX",
  );

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
  // TWENTY-TWO, which is every preset `ShapeGeometry` models and the renderer
  // draws. It was seven, hand-written, while fifteen more were modeled, painted
  // and unofferable; `shape_catalogue.test.mjs` is what keeps the two in step.
  await expect(gallery.locator("[data-shape-geometry]")).toHaveCount(22);

  await gallery.locator('[data-shape-geometry="ellipse"]').click();
  // Word ARMS the pointer rather than inserting: the next gesture on the page
  // decides where and how big. A bare click still places the default size.
  await expect(page.locator("body")).toHaveAttribute("data-shape-draw", "ellipse");
  await clickOnPage(page, 0.3, 0.3);
  await expect(page.locator("#status")).toContainText("Oval added");

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
  await page.locator('#shapeGalleryMenu [data-shape-geometry="rect"]').click();
  await clickOnPage(page, 0.3, 0.3);

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
    // `canRotate` joins them: a shape leaf carries its own `a:xfrm` and the
    // anchor pass paints it, so the subject is rotatable even though the group
    // root that owns its resize grips is not.
    "canResize,canRotate,canMove,canWrap,canDelete,canFill,canStroke",
  );

  // The subject owns Fill/Stroke while the stable reference's root owns resize,
  // move, wrap, and delete. Every group handle commits root extent + transform +
  // anchor through one exact-inverse engine transaction.
  await expect(page.locator(".overlay .object-handle:not(.object-rotate-handle)")).toHaveCount(8);
  // The rotation grip rides alongside the eight, so the resize count is taken
  // on the resize grips rather than on every `.object-handle`.
  await expect(page.locator(".overlay .object-rotate-handle")).toHaveCount(1);
  const bar = page.locator(".object-context-bar");
  await expect(bar).toContainText("handles to resize");
  await expect(bar.getByRole("button", { name: "Shape fill" })).toBeVisible();
  await expect(bar.getByRole("button", { name: "Shape outline" })).toBeVisible();
  await expect(bar.getByRole("button", { name: "Edit alt text" })).toHaveCount(0);
  await expect(bar.getByRole("button", { name: "Delete object" })).toBeVisible();
  await expect(page.locator(".object-wrap-btn")).toHaveCount(7);

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
  await page.locator('#shapeGalleryMenu [data-shape-geometry="rect"]').click();
  await clickOnPage(page, 0.3, 0.3);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "shape");

  // A PLACED shape is two undoable actions, and this is the honest guard for
  // that: `insertShape` authors a fixed 2"x1" at the caret's paragraph and the
  // drawn (or clicked) geometry is committed by the resize that follows, because
  // the facade has no insert-with-geometry call. The first undo takes the
  // geometry back, the second takes the shape away, and nothing is left half
  // made. Collapsing the two needs an engine operation, not a host change.
  await page.keyboard.press(`${MOD}+z`);
  await page.keyboard.press(`${MOD}+z`);
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


// ---- Drawing a shape, which is how both competitors place one ---------------

test("dragging on the page draws the picked shape AT THE DRAGGED SIZE", async ({
  page,
  consoleErrors,
}) => {
  // The gesture, not the affordance: a numeric default where the competitor has
  // a direct-manipulation drag is the mistake the crop dialog already made here.
  await open(page);
  await openInsertTab(page);
  await page.locator("#insertShapeBtn").click();
  await page.locator('#shapeGalleryMenu [data-shape-geometry="roundRect"]').click();
  await expect(page.locator("body")).toHaveAttribute("data-shape-draw", "roundRect");

  const { box } = await dragOnPage(page, { x: 0.2, y: 0.2 }, { x: 0.7, y: 0.5 });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "shape");

  // The DOCUMENT holds a rounded rectangle of the size that was dragged — not
  // the 2"x1" `insertShape` authors. Asked of the engine's own placed rect, in
  // screen pixels, so this cannot pass by agreeing with itself.
  const outline = await page.locator(".overlay .object-outline").first().boundingBox();
  const wantedW = box.width * 0.5;
  const wantedH = box.height * 0.3;
  expect(Math.abs(outline.width - wantedW)).toBeLessThan(wantedW * 0.12);
  expect(Math.abs(outline.height - wantedH)).toBeLessThan(wantedH * 0.15);
  // And it is the preset that was picked, not the default rectangle.
  await expect(page.locator("#status")).toContainText("Rounded rectangle added");

  // The mode is over: the next press is an ordinary caret placement.
  await expect(page.locator("body")).not.toHaveAttribute("data-shape-draw", /./);

  expect(consoleErrors).toEqual([]);
});

test("Escape leaves the draw mode and places nothing", async ({ page, consoleErrors }) => {
  await open(page);
  await openInsertTab(page);
  await page.locator("#insertShapeBtn").click();
  await page.locator('#shapeGalleryMenu [data-shape-geometry="star5"]').click();
  await expect(page.locator("body")).toHaveAttribute("data-shape-draw", "star5");

  await page.keyboard.press("Escape");
  await expect(page.locator("body")).not.toHaveAttribute("data-shape-draw", /./);
  await expect(page.locator("#status")).toContainText("Drawing cancelled");
  // Nothing was created: a click now places a caret, not a star.
  await clickOnPage(page, 0.3, 0.3);
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-mode", "selected");

  expect(consoleErrors).toEqual([]);
});

test("double-clicking a shape gives it text and puts the caret inside", async ({
  page,
  consoleErrors,
}) => {
  // Word's and Docs' gesture. `addTextToShape` turns the `wps:wsp` into a
  // text-bearing shape — the preset, its guides, its fill and its outline are
  // kept — so a star stays a star and gets words inside it.
  await open(page);
  await openInsertTab(page);
  await page.locator("#insertShapeBtn").click();
  await page.locator('#shapeGalleryMenu [data-shape-geometry="star5"]').click();
  await dragOnPage(page, { x: 0.2, y: 0.2 }, { x: 0.6, y: 0.5 });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "shape");

  // The document's own counts are the evidence: the a11y mirror does not carry a
  // SHAPE's text body (only a text box's), so asserting through it would be
  // asserting the mirror's coverage rather than the edit.
  const number = async (id) =>
    Number.parseInt((await page.locator(id).textContent()).replace(/[^0-9]/g, ""), 10);
  const counts = async () => ({
    words: await number("#statWords"),
    paragraphs: await number("#statParas"),
  });
  const before = await counts();
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await page.mouse.dblclick(box.x + box.width * 0.4, box.y + box.height * 0.35);
  await page.waitForTimeout(400);

  // The shape gained a text body — a real node in the document, which is what
  // `addTextToShape` writes; the star keeps its preset, its fill and its outline.
  const gained = await counts();
  expect(gained.paragraphs, "the shape must have gained a text body").toBeGreaterThan(
    before.paragraphs,
  );

  await page.keyboard.type("INSIDE THE STAR");
  await page.waitForTimeout(300);
  const typed = await counts();
  expect(typed.words, "the typing must have landed in the document").toBeGreaterThanOrEqual(
    gained.words + 3,
  );

  expect(consoleErrors).toEqual([]);
});
