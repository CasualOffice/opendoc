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
import { test, expect, gotoEditor } from "./fixtures.mjs";

const GROUPED = "../fixtures/generated/grouped-text-boxes.docx";
const FLOATING = "../fixtures/generated/floating-text-box.docx";

async function open(page, file) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(file);
  const canvas = page.locator(".page-wrap .page").first();
  await expect(canvas).toBeVisible();
  await expect.poll(async () => (await canvas.boundingBox())?.width ?? 0).toBeGreaterThan(0);
  // The imported document has replaced the fixture's content.
  await expect(page.locator("#a11yDocument")).toContainText("Body before");
  return canvas.boundingBox();
}

// Finds a selectable object by scanning, rather than hard-coding a point that
// silently stops meaning anything when the layout shifts.
async function findObject(page, box) {
  for (let fy = 0.05; fy < 0.4; fy += 0.02) {
    for (let fx = 0.1; fx < 0.85; fx += 0.06) {
      await page.mouse.click(box.x + box.width * fx, box.y + box.height * fy);
      const kind = await page.locator("#pages").getAttribute("data-object-kind");
      if (kind) return { x: box.x + box.width * fx, y: box.y + box.height * fy, kind };
    }
  }
  return null;
}

test("a text box inside a group is selectable", async ({ page, consoleErrors }) => {
  const box = await open(page, GROUPED);

  // Group children carried no identity in the layout, so this found nothing at
  // all — the content was visible and completely inert.
  const found = await findObject(page, box);
  expect(found, "a grouped object should be selectable").not.toBeNull();
  expect(found.kind).toBe("textbox");

  expect(consoleErrors).toEqual([]);
});

test("a grouped text box can be entered and edited, leaving the body alone", async ({
  page,
  consoleErrors,
}) => {
  const box = await open(page, GROUPED);
  const found = await findObject(page, box);
  expect(found).not.toBeNull();
  const bodyBefore = await page.locator("#a11yDocument").textContent();

  await page.mouse.dblclick(found.x, found.y);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  await page.keyboard.type("NESTED");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  // It went into the grouped box, not the document body.
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

  expect(consoleErrors).toEqual([]);
});

test("a floating text box can be entered and edited", async ({ page, consoleErrors }) => {
  const box = await open(page, FLOATING);
  const found = await findObject(page, box);
  expect(found).not.toBeNull();
  expect(found.kind).toBe("textbox");
  const bodyBefore = await page.locator("#a11yDocument").textContent();

  await page.mouse.dblclick(found.x, found.y);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
  await page.keyboard.type("FLOAT");

  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

  expect(consoleErrors).toEqual([]);
});
