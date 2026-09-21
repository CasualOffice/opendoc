// Clicking a shape inside a group — the gesture that actually gets performed.
//
// `docs/117`. The owner reported grouped drawings as uneditable three times.
// #556 gave grouped children model identity; #574 made Tab walk them. Neither
// changed what they experience, because a click on a grouped shape selected
// the whole group — every time, however many times they clicked. `objectAt`
// answers with the group root by design, and nothing ever asked a different
// question. Double-click jumped straight into typing, skipping child selection
// entirely, so no gesture at all selected one shape.
//
// Word's grammar, adopted in `docs/117` §5: first click takes the group, the
// next click reaches inside it, Escape climbs back out one level.
import { test, expect, gotoEditor, stableBox } from "./fixtures.mjs";

const NESTED = "../fixtures/generated/nested-group.docx";

async function openNested(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(NESTED);
  await expect(page.locator("#a11yDocument")).toContainText(
    "Body after the nested group",
  );
  return stableBox(page.locator(".page-wrap .page").first());
}

const state = (page) =>
  page.locator("#pages").evaluate((el) => ({
    kind: el.dataset.objectKind ?? null,
    path: el.dataset.objectPath ?? null,
    mode: el.dataset.objectMode ?? null,
  }));

/** Clicks across the sheet until the group answers, and returns that point. */
async function findGroup(page, box) {
  for (let fy = 0.03; fy < 0.5; fy += 0.02) {
    for (let fx = 0.05; fx < 0.9; fx += 0.04) {
      const at = { x: box.x + box.width * fx, y: box.y + box.height * fy };
      await page.mouse.click(at.x, at.y);
      const now = await state(page);
      if (now.kind === "group") return at;
    }
  }
  return null;
}

/** Every distinct child path reachable by a second click, swept across the
 *  group's OWN box.
 *
 *  Read from the selection outline the product draws rather than guessed as a
 *  fraction of the page: a fraction stops meaning anything the moment the
 *  layout shifts, and a sweep that misses the shapes would pass this test by
 *  reaching nothing. */
async function childrenReachableByClick(page, seed) {
  const outline = await page
    .locator(".overlay .object-outline")
    .first()
    .boundingBox();
  expect(
    outline,
    "a selected group must draw an outline to sweep",
  ).not.toBeNull();
  const found = new Set();
  const steps = 7;
  for (let i = 0; i <= steps; i++) {
    for (let j = 0; j <= steps; j++) {
      await page.keyboard.press("Escape");
      await page.keyboard.press("Escape");
      await page.mouse.click(seed.x, seed.y); // 1st click: the group
      await page.mouse.click(
        outline.x + (outline.width * (i + 0.5)) / (steps + 1),
        outline.y + (outline.height * (j + 0.5)) / (steps + 1),
      );
      const now = await state(page);
      if (now.path) found.add(now.path);
    }
  }
  return found;
}

/** A point where a SECOND click reaches a child — i.e. a point genuinely over
 *  a shape inside the group, not over the group's own background. Found by
 *  sweeping the group's outline, so it survives a layout change. */
async function pointOverAChild(page, seed) {
  const outline = await page
    .locator(".overlay .object-outline")
    .first()
    .boundingBox();
  expect(
    outline,
    "a selected group must draw an outline to sweep",
  ).not.toBeNull();
  const steps = 7;
  for (let i = 0; i <= steps; i++) {
    for (let j = 0; j <= steps; j++) {
      const at = {
        x: outline.x + (outline.width * (i + 0.5)) / (steps + 1),
        y: outline.y + (outline.height * (j + 0.5)) / (steps + 1),
      };
      await page.keyboard.press("Escape");
      await page.keyboard.press("Escape");
      await page.mouse.click(seed.x, seed.y);
      await page.mouse.click(at.x, at.y);
      const now = await state(page);
      if (now.path) return { at, path: now.path };
    }
  }
  return null;
}

test("the first click takes the group, the second takes the shape under it", async ({
  page,
  consoleErrors,
}) => {
  const box = await openNested(page);
  const seed = await findGroup(page, box);
  expect(seed, "the group should be selectable").not.toBeNull();

  // A point that is genuinely over a child, established first so the two
  // assertions below are about the SAME pixel. Without that, "the first click
  // takes the group" can be satisfied by a point over the group's background,
  // and a fix that always descended would pass it.
  const child = await pointOverAChild(page, seed);
  expect(child, "the group should contain a clickable shape").not.toBeNull();

  // 1. From nothing, one click on that pixel takes the GROUP — `docs/117` §5
  //    rule 1, which both competitors agree on and which keeps a group
  //    pickable, movable and deletable as one thing.
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  expect((await state(page)).kind, "the test must start deselected").toBeNull();
  await page.mouse.click(child.at.x, child.at.y);
  expect(
    await state(page),
    "the FIRST click must take the group whole",
  ).toMatchObject({
    kind: "group",
    path: "",
    mode: "selected",
  });

  // 2. The next click on the same pixel reaches the shape — Word's second
  //    click, and the gesture that used to do nothing at all.
  await page.mouse.click(child.at.x, child.at.y);
  const now = await state(page);
  expect(now.mode).toBe("selected");
  expect(now.path, "the second click must reach a child, not the group").toBe(
    child.path,
  );

  expect(consoleErrors).toEqual([]);
});

test("a nested shape is reached by the same click, not a special gesture", async ({
  page,
  consoleErrors,
}) => {
  // The owner's Medical form is a group of four with two inside a nested
  // group, and "nested" was called out separately in the report. A dotted path
  // is the only evidence the chrome ever sees that the nesting exists.
  const box = await openNested(page);
  const seed = await findGroup(page, box);
  expect(seed).not.toBeNull();

  const reachable = await childrenReachableByClick(page, seed);
  expect(
    [...reachable].some((path) => path.includes(".")),
    `no nested child was reachable by clicking; reached: ${[...reachable].join(", ")}`,
  ).toBe(true);
  expect(
    [...reachable].filter((path) => path !== "").length,
    `only one shape was reachable: ${[...reachable].join(", ")}`,
  ).toBeGreaterThan(1);

  expect(consoleErrors).toEqual([]);
});

test("Escape climbs to the group, then out", async ({
  page,
  consoleErrors,
}) => {
  // Without this, descending was a one-way door: the group just picked up was
  // gone and reaching it again meant clicking from scratch.
  const box = await openNested(page);
  const seed = await findGroup(page, box);
  expect(seed).not.toBeNull();
  await page.mouse.click(seed.x, seed.y);
  expect((await state(page)).path).not.toBe("");

  await page.keyboard.press("Escape");
  expect(
    await state(page),
    "Escape from a child selects its group",
  ).toMatchObject({
    kind: "group",
    path: "",
  });

  await page.keyboard.press("Escape");
  expect((await state(page)).kind, "Escape from the group leaves").toBeNull();

  expect(consoleErrors).toEqual([]);
});

test("double-click still goes straight into a grouped text box", async ({
  page,
  consoleErrors,
}) => {
  // `docs/117` §5 rule 5: the fast path is unchanged by the new second click.
  const box = await openNested(page);
  const seed = await findGroup(page, box);
  expect(seed).not.toBeNull();
  await page.keyboard.press("Escape");

  await page.mouse.dblclick(seed.x, seed.y);
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-mode",
    "editing",
  );
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  expect(consoleErrors).toEqual([]);
});
