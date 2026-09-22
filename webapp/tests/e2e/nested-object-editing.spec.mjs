// Editing objects that live inside a shape group, and floating text boxes.
//
// A grouped object RENDERED but was completely unreachable: `PlacedAnchor.node`
// was `None` for every group child, so nothing could map a click back to the
// model — not selection, not entry, not editing. Four layers had to agree:
// identity in the layout, admission into the selectable set, resolution for the
// ops, and caret geometry for anchored (not inline) text-box content.
//
// No fixture had a group or a floating text box, which is why this went
// unverified. Both are built for these tests.
import {
  test,
  expect,
  gotoEditor,
  stableBox,
  mirrorBlocks,
  expectTypedIntoOneBlock,
} from "./fixtures.mjs";

const GROUPED = "../fixtures/generated/grouped-text-boxes.docx";
const FLOATING = "../fixtures/generated/floating-text-box.docx";

async function open(page, file) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(file);
  const canvas = page.locator(".page-wrap .page").first();
  await expect(canvas).toBeVisible();
  const box = await stableBox(canvas);
  // The imported document has replaced the fixture's content.
  await expect(page.locator("#a11yDocument")).toContainText("Body before");
  return box;
}

// Finds a selectable object by scanning, rather than hard-coding a point that
// silently stops meaning anything when the layout shifts.
async function findObject(page, box) {
  for (let fy = 0.05; fy < 0.4; fy += 0.02) {
    for (let fx = 0.1; fx < 0.85; fx += 0.06) {
      await page.mouse.click(box.x + box.width * fx, box.y + box.height * fy);
      const kind = await page
        .locator("#pages")
        .getAttribute("data-object-kind");
      if (kind)
        return { x: box.x + box.width * fx, y: box.y + box.height * fy, kind };
    }
  }
  return null;
}

test("a text box inside a group is reachable through explicit descent", async ({
  page,
  consoleErrors,
}) => {
  const box = await open(page, GROUPED);
  const pages = page.locator("#pages");

  // The first click selects the group as one structural object. Enter then
  // descends to its first paint-order child without losing the stable root.
  const found = await findObject(page, box);
  expect(found, "a grouped object should be selectable").not.toBeNull();
  expect(found.kind).toBe("group");
  const root = await pages.getAttribute("data-object-root");
  expect(root).toMatch(/^[0-9a-f]{32}$/);
  await expect(pages).toHaveAttribute("data-object-subject", root);
  await expect(pages).toHaveAttribute("data-object-path", "");

  await page.keyboard.press("Enter");
  await expect(pages).toHaveAttribute("data-object-kind", "textbox");
  await expect(pages).toHaveAttribute("data-object-root", root);
  await expect(pages).not.toHaveAttribute("data-object-subject", root);
  await expect(pages).toHaveAttribute("data-object-path", /\d+(\.\d+)*/);

  expect(consoleErrors).toEqual([]);
});

test("a grouped text box can be entered and edited, leaving the body alone", async ({
  page,
  consoleErrors,
}) => {
  const box = await open(page, GROUPED);
  const found = await findObject(page, box);
  expect(found).not.toBeNull();
  const blocksBefore = await mirrorBlocks(page);

  await page.mouse.dblclick(found.x, found.y);
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );
  // Entering re-renders; a key pressed mid-render is dropped.
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  await page.keyboard.type("NESTED");
  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Typing",
  );
  // It went into the grouped box, not the document body.
  expectTypedIntoOneBlock(
    blocksBefore,
    await mirrorBlocks(page),
    "NESTED",
  );

  expect(consoleErrors).toEqual([]);
});

test("a grouped text box keeps inside clicks and Escape climbs out one level at a time", async ({
  page,
  consoleErrors,
}) => {
  const box = await open(page, GROUPED);
  const found = await findObject(page, box);
  expect(found).not.toBeNull();
  const blocksBefore = await mirrorBlocks(page);

  await page.mouse.dblclick(found.x, found.y);
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  // Resolve the grouped child's extent from the editor's own selection chrome,
  // not a guessed page fraction. Escape once selects the object and exposes the
  // exact outline used by the product.
  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "selected",
  );
  const outline = page.locator(".overlay .object-outline");
  await expect(outline).toBeVisible();
  const rect = await outline.boundingBox();
  expect(rect).not.toBeNull();

  // Re-enter near the text, then click the empty far side. A missing glyph hit
  // inside the placed box must resolve within the active box, not eject the
  // caret to the document body.
  await page.mouse.dblclick(
    rect.x + rect.width * 0.15,
    rect.y + rect.height * 0.5,
  );
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );
  await page.mouse.click(
    rect.x + rect.width * 0.85,
    rect.y + rect.height * 0.75,
  );
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );
  await page.keyboard.type("INSIDE");
  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Typing",
  );
  expectTypedIntoOneBlock(
    blocksBefore,
    await mirrorBlocks(page),
    "INSIDE",
  );

  // Escape climbs one level per press, which for a shape INSIDE a group is
  // three: editing → that shape → its group → the document (`docs/117` §5
  // rule 4). It used to drop from the shape straight to the document, so the
  // group the user had just entered was gone and re-selecting it meant
  // clicking again from scratch.
  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "selected",
  );
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-kind",
    "textbox",
  );

  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-kind",
    "group",
  );
  await expect(page.locator("#pages")).toHaveAttribute("data-object-path", "");

  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).not.toHaveAttribute(
    "data-object-mode",
    /.*/,
  );

  expect(consoleErrors).toEqual([]);
});

test("clicking the body exits a grouped text box and sends typing to the body", async ({
  page,
  consoleErrors,
}) => {
  const box = await open(page, GROUPED);
  const found = await findObject(page, box);
  expect(found).not.toBeNull();

  await page.mouse.dblclick(found.x, found.y);
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  // The grouped fixture keeps its objects in the upper part of the page. This
  // target is derived from the live page rectangle and is below that object
  // search region, in ordinary body content.
  const target = { x: box.x + box.width * 0.5, y: box.y + box.height * 0.55 };
  expect(
    await page.evaluate(
      (point) => document.elementFromPoint(point.x, point.y) !== null,
      target,
    ),
    "the click-away point must be inside the browser viewport",
  ).toBe(true);
  await page.mouse.click(target.x, target.y);
  await expect(page.locator("#pages")).not.toHaveAttribute(
    "data-object-mode",
    "editing",
  );

  await page.keyboard.type("BODYEXIT");
  await expect(page.locator("#a11yDocument")).toContainText("BODYEXIT");

  expect(consoleErrors).toEqual([]);
});

test("a floating text box can be entered and edited", async ({
  page,
  consoleErrors,
}) => {
  const box = await open(page, FLOATING);
  const found = await findObject(page, box);
  expect(found).not.toBeNull();
  expect(found.kind).toBe("textbox");
  const blocksBefore = await mirrorBlocks(page);

  await page.mouse.dblclick(found.x, found.y);
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );
  await page.keyboard.type("FLOAT");

  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Typing",
  );
  expectTypedIntoOneBlock(
    blocksBefore,
    await mirrorBlocks(page),
    "FLOAT",
  );

  expect(consoleErrors).toEqual([]);
});
