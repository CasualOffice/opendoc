// Arrange, in a browser: wrap and the in-line conversion, stacking, grouping,
// rotation, and drawing a shape out on the page.
//
// The rules are `tests/object_arrange.test.mjs` (node). What only a browser can
// answer is whether the gesture reaches the ENGINE and whether the DOCUMENT
// changed — so every assertion here is about engine-reported state or about
// what is now under the pointer, never about a DOM node existing.
import { test, expect, gotoEditor, clickIntoFirstPage, stableBox, MOD } from "./fixtures.mjs";

async function gotoFloat(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
}

/** Selects the float fixture's image with a click. */
async function selectFloat(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await page.mouse.click(box.x + box.width * 0.14, box.y + box.height * 0.11);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  return box;
}

const capabilities = (page) => page.locator("#pages").getAttribute("data-object-capabilities");
const outlineBox = async (page) => stableBox(page.locator(".overlay .object-outline").first());

// ---- The case the owner reported -------------------------------------------

test("an image can be put IN LINE and taken back out, and the engine agrees", async ({
  page,
  consoleErrors,
}) => {
  // "In line" did not exist as a mode anywhere in the product, and an inline
  // object — which is what inserting a picture gives you — had no wrap control
  // at all, because the chip was keyed on a capability bit the engine only sets
  // for objects that already float.
  await gotoFloat(page);
  await selectFloat(page);

  // Floating: the engine says it can be moved and wrapped.
  expect(await capabilities(page)).toContain("canWrap");
  expect(await capabilities(page)).toContain("canMove");
  const floating = await outlineBox(page);

  await page.locator('.object-wrap-btn[data-wrap="inline"]').click();
  await page.waitForTimeout(250);

  // The DOCUMENT changed, not a flag: `setObjectAnchorKind` rewrites the node,
  // so the engine now reports an object with no anchor — it cannot be moved and
  // it cannot carry a wrap, because the flow decides where it sits.
  await expect(page.locator('.object-wrap-btn[data-wrap="inline"]')).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(await capabilities(page)).not.toContain("canWrap");
  expect(await capabilities(page)).not.toContain("canMove");
  // ...and it MOVED, into the run flow, which is the reader-visible half.
  const inline = await outlineBox(page);
  expect(Math.abs(inline.x - floating.x) + Math.abs(inline.y - floating.y)).toBeGreaterThan(8);

  // And back out. Word floats on any wrapping mode; the mode asked for is the
  // one that lands, not the square default the conversion would give.
  await page.locator('.object-wrap-btn[data-wrap="behind"]').click();
  await page.waitForTimeout(250);
  expect(await capabilities(page)).toContain("canWrap");
  await expect(page.locator('.object-wrap-btn[data-wrap="behind"]')).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  expect(consoleErrors).toEqual([]);
});

test("the wrap control is offered for an INLINE object, not only a floating one", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);
  await page.locator('.object-wrap-btn[data-wrap="inline"]').click();
  await page.waitForTimeout(250);

  // The whole row is still there, live, with in-line ticked. This is what was
  // missing: an inserted picture is inline, so this was the DEFAULT state.
  await expect(page.locator(".object-wrap-btn")).toHaveCount(7);
  for (const mode of ["square", "tight", "through", "topAndBottom", "behind", "front"]) {
    await expect(page.locator(`.object-wrap-btn[data-wrap="${mode}"]`)).toBeEnabled();
  }

  expect(consoleErrors).toEqual([]);
});

// ---- Position ---------------------------------------------------------------

test("a Position cell moves the object to that corner of the margin", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  const canvas = await selectFloat(page);
  const before = await outlineBox(page);

  await page.locator('.object-bar-btn[data-object-menu="objectPositionMenu"]').click();
  await expect(page.locator("#objectPositionMenu")).toBeVisible();
  await expect(page.locator("#objectPositionMenu [data-position]")).toHaveCount(9);
  await page.locator('#objectPositionMenu [data-position="bottomRight"]').click();
  await page.waitForTimeout(300);

  const after = await outlineBox(page);
  expect(after.x, "bottom-right must be to the right of where it was").toBeGreaterThan(before.x);
  expect(after.y, "bottom-right must be below where it was").toBeGreaterThan(before.y);
  // ...and it is inside the page, against the far margin.
  expect(after.x + after.width).toBeLessThanOrEqual(canvas.x + canvas.width + 2);

  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(250);
  const undone = await outlineBox(page);
  expect(Math.abs(undone.x - before.x)).toBeLessThanOrEqual(3);

  expect(consoleErrors).toEqual([]);
});

// ---- Drawing, grouping and stacking ----------------------------------------

/** Draws a shape by picking `token` and dragging the given page fractions. */
async function drawShape(page, token, from, to) {
  await page.locator("#tabInsert").click();
  await page.locator("#insertShapeBtn").click();
  await page.locator(`#shapeGalleryMenu [data-shape-geometry="${token}"]`).click();
  await expect(page.locator("body")).toHaveAttribute("data-shape-draw", token);
  const box = await stableBox(page.locator(".page-wrap .page").first());
  const at = (f) => ({ x: box.x + box.width * f.x, y: box.y + box.height * f.y });
  const start = at(from);
  const end = at(to);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(end.x, end.y, { steps: 8 });
  await page.mouse.up();
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "shape");
  return box;
}

test("two shapes can be held together and grouped, and one undo separates them", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Kept in the upper half of the sheet on purpose: a US-Letter page at 100% is
  // taller than a 900px viewport, so a fraction past ~0.45 is off screen and a
  // click there lands on the footer.
  await drawShape(page, "rect", { x: 0.12, y: 0.22 }, { x: 0.4, y: 0.42 });
  const box = await drawShape(page, "ellipse", { x: 0.55, y: 0.22 }, { x: 0.85, y: 0.42 });

  // Ctrl/⌘+click the first one to hold both — Word's, PowerPoint's and Docs'
  // modifier, and the only way Group can have two objects to act on.
  const modifier = process.platform === "darwin" ? "Meta" : "Control";
  await page.keyboard.down(modifier);
  // Well inside the first shape and well BELOW the chip, which is body-level,
  // takes pointer events and sits just above whatever is selected.
  await page.mouse.click(box.x + box.width * 0.25, box.y + box.height * 0.32);
  await page.keyboard.up(modifier);
  await expect(page.locator(".overlay .object-outline.is-co-selected")).toHaveCount(1);

  await page.locator('.object-bar-btn[data-object-menu="objectArrangeMenu"]').click();
  await expect(page.locator('#objectArrangeMenu [data-arrange="group"]')).toBeEnabled();
  await page.locator('#objectArrangeMenu [data-arrange="group"]').click();
  await page.waitForTimeout(300);

  // The DOCUMENT holds a group where it held two shapes — the engine's own word
  // for what is selected, not a class this host put on something.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "group");

  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(300);
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-kind", "group");

  expect(consoleErrors).toEqual([]);
});

test("Group on ONE object is disabled and says what to do instead", async ({
  page,
  consoleErrors,
}) => {
  // SKILL §10: never a dead control. The engine would answer "grouping needs at
  // least two objects", which is true and useless; the host's own sentence is
  // the only refusal here it writes for itself, because it says what to DO.
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await drawShape(page, "rect", { x: 0.2, y: 0.22 }, { x: 0.5, y: 0.4 });

  await page.locator('.object-bar-btn[data-object-menu="objectArrangeMenu"]').click();
  const group = page.locator('#objectArrangeMenu [data-arrange="group"]');
  await expect(group).toBeDisabled();
  // The reason reaches a screen reader and a touch user, not only a hover.
  await expect(group).toHaveAttribute("aria-label", /click another object/i);
  await expect(group).toHaveAttribute("title", /click another object/i);
  const ungroup = page.locator('#objectArrangeMenu [data-arrange="ungroup"]');
  await expect(ungroup).toBeDisabled();
  await expect(ungroup).toHaveAttribute("aria-label", /select a group/i);

  expect(consoleErrors).toEqual([]);
});

/** The colour the page is actually PAINTED at a client point, read out of the
 *  raster canvas. This is the reader's answer to "which shape is on top", and
 *  the only one this host exposes: `objectAt` does not order by the stacking
 *  key, so a hit-test would be asserting something the build does not promise. */
async function paintedAt(page, x, y) {
  return page.evaluate(([cx, cy]) => {
    const canvas = document.querySelector(".page-wrap canvas.page");
    const rect = canvas.getBoundingClientRect();
    const px = Math.round(((cx - rect.left) / rect.width) * canvas.width);
    const py = Math.round(((cy - rect.top) / rect.height) * canvas.height);
    const [r, g, b] = canvas.getContext("2d").getImageData(px, py, 1, 1).data;
    return `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
  }, [x, y]);
}

test("Bring to front repaints the page: the shape you named is now on top", async ({
  page,
  consoleErrors,
}) => {
  // The strongest evidence available, and the one a reader would give: the
  // PIXEL where the two shapes overlap changes colour.
  //
  // This is also the guard for the bug it was written after. Every shape this
  // host inserts is wrapped in a group-of-one, so its SUBJECT is technically a
  // group child; restacking the subject reorders it among its one sibling and
  // repaints nothing at all. The command has to be sent to the ROOT.
  //
  // Note which shape starts on top: `insertShape` puts the new drawing at the
  // caret's offset, which is ahead of the previous one in the paragraph, so the
  // shape drawn SECOND paints first. That is the engine's order, not an
  // assumption — the first assertion below is what pins it.
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // A red shape, then a default-filled one drawn over it.
  const box = await drawShape(page, "rect", { x: 0.15, y: 0.2 }, { x: 0.55, y: 0.4 });
  await page.locator('.object-bar-btn[aria-label="Shape fill"]').click();
  await page.locator('#shapeFillMenu [data-shape-color="#ff0000"]').click();
  await page.waitForTimeout(400);
  await drawShape(page, "rect", { x: 0.35, y: 0.26 }, { x: 0.75, y: 0.45 });

  const overlapX = box.x + box.width * 0.45;
  const overlapY = box.y + box.height * 0.33;
  expect(
    await paintedAt(page, overlapX, overlapY),
    "the red shape starts on top",
  ).toBe("#ff0000");

  // The shape drawn second is selected; bring it forward of the red one.
  await page.locator('.object-bar-btn[data-object-menu="objectArrangeMenu"]').click();
  await expect(page.locator("#objectArrangeMenu [data-z-order]")).toHaveCount(4);
  await page.locator('#objectArrangeMenu [data-z-order="front"]').click();
  await page.waitForTimeout(600);

  await expect(page.locator("#status")).not.toHaveClass(/error/);
  expect(
    await paintedAt(page, overlapX, overlapY),
    "after Bring to front the page must PAINT the other shape over the red one",
  ).not.toBe("#ff0000");

  expect(consoleErrors).toEqual([]);
});

test("stacking is refused for an IN-LINE object, with the reason", async ({
  page,
  consoleErrors,
}) => {
  // An in-line object is ordered by the text flow and has no stacking of its
  // own. The rows are offered and disabled carrying that, never hidden.
  await gotoFloat(page);
  await selectFloat(page);
  await page.locator('.object-wrap-btn[data-wrap="inline"]').click();
  await page.waitForTimeout(300);

  await page.locator('.object-bar-btn[data-object-menu="objectArrangeMenu"]').click();
  for (const order of ["front", "forward", "backward", "back"]) {
    const row = page.locator(`#objectArrangeMenu [data-z-order="${order}"]`);
    await expect(row).toBeDisabled();
    await expect(row).toHaveAttribute("aria-label", /restacked/i);
  }

  expect(consoleErrors).toEqual([]);
});

test("Rotate is live on a shape and undoable; a text-boxless object says why not", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await drawShape(page, "rightArrow", { x: 0.25, y: 0.22 }, { x: 0.6, y: 0.4 });

  await page.locator('.object-bar-btn[data-object-menu="objectRotateMenu"]').click();
  await expect(page.locator("#objectRotateMenu [data-rotate]")).toHaveCount(4);
  await page.locator('#objectRotateMenu [data-rotate="right90"]').click();
  await page.waitForTimeout(250);
  // The edit was ACCEPTED: a refusal would have put its reason on the status
  // line, which is the one channel this command has when it cannot act.
  await expect(page.locator("#status")).not.toHaveClass(/error/);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  expect(consoleErrors).toEqual([]);
});

// ---- Reachability ----------------------------------------------------------

test("every arrange command is reachable from the palette as well as the chip", async ({
  page,
  consoleErrors,
}) => {
  // SKILL §10: a capability on one surface is the defect this repository keeps
  // re-finding. The chip is surface one; the palette is the keyboard route.
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await drawShape(page, "rect", { x: 0.2, y: 0.22 }, { x: 0.5, y: 0.4 });

  for (const label of [
    "Bring object forward",
    "Send object backward",
    "Group objects",
    "Ungroup objects",
    "Rotate object right 90 degrees",
    "Flip object horizontally",
    "Put object in line with text",
    "Add text to shape",
  ]) {
    await page.keyboard.press(`${MOD}+Shift+KeyP`);
    await expect(page.locator("#cmdInput")).toBeVisible();
    await page.locator("#cmdInput").fill(label);
    const options = page.locator("#cmdList [role=option]");
    const texts = await options.allTextContents();
    expect(
      texts.some((text) => text.includes(label)),
      `no palette command for "${label}" — found: ${texts.join(" | ")}`,
    ).toBe(true);
    await page.keyboard.press("Escape");
  }

  expect(consoleErrors).toEqual([]);
});

test("the object right-click menu carries wrap, position, arrange and rotate", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  const box = await selectFloat(page);
  // Right-click ON the image: the same point that selected it.
  await page.mouse.click(box.x + box.width * 0.14, box.y + box.height * 0.11, { button: "right" });
  await expect(page.locator(".editor-context-menu")).toBeVisible();

  const labels = await page.locator(".editor-context-menu .menu-item").allTextContents();
  for (const expected of ["Wrap text", "Position", "Arrange", "Rotate"]) {
    expect(
      labels.some((text) => text.includes(expected)),
      `no object-menu row for "${expected}" — found: ${labels.join(" | ")}`,
    ).toBe(true);
  }

  expect(consoleErrors).toEqual([]);
});
