// The editing gestures a Word or Docs user makes on a picture, made the way they
// make them — the pointer and the keyboard, never an engine call.
//
// Each test is a row of `docs/109` and was driven red by putting its defect
// back (the mutation and the red output are in the commit that added it):
//
//   HF-166  a floating logo inside a field result or a content control painted,
//           and a click on it dropped a caret into the text behind it;
//   HF-214  a picture that is a member of a group could not be given alt text;
//   HF-252  a picture could not be replaced in place;
//   HF-254  a picture's border had no control;
//   HF-106  in crop mode the arrows moved the PICTURE, and a cancelled grip drag
//           left the crop following the pointer;
//   HF-259  dragging an in-line picture did nothing and said nothing;
//   HF-058  the object chip stayed on screen after its object scrolled away;
//   HF-059  ⌘/Ctrl+V over a copied image pasted nothing;
//   and Word's Ctrl+Arrow, which nudges a selected object by a pixel rather
//   than dropping the selection to move a caret nobody can see.
import { MOD, WORD_MOD, expect, gotoEditor, stableBox, test } from "./fixtures.mjs";

const WRAPPED = "../fixtures/generated/wrapped-pictures.docx";

/** The pictures' page-relative frames, in inches, as the fixture generator
 *  (`fixtures/tools/make_wrapped_pictures.py`) writes them. */
const FIELD_LOGO = { x: 1.75, y: 1.875 };
const CONTROL_LOGO = { x: 5.25, y: 1.875 };
const PARTNER_B = { x: 3.25, y: 4.375 };

/** Opens the fixture and returns a function from page inches to a client point. */
async function openWrapped(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(WRAPPED);
  await expect(page.locator("#a11yDocument")).toContainText("Inline field picture");
  const sheet = await stableBox(page.locator(".page-wrap .page").first());
  return ({ x, y }) => ({ x: sheet.x + (x * sheet.width) / 8.5, y: sheet.y + (y * sheet.height) / 11 });
}

const objectState = (page) =>
  page.locator("#pages").evaluate((el) => ({
    kind: el.dataset.objectKind ?? null,
    mode: el.dataset.objectMode ?? null,
    node: el.dataset.objectSelected ?? null,
    path: el.dataset.objectPath ?? null,
    outline: el.dataset.shapeOutline ?? null,
  }));

const outlineBox = (page) => stableBox(page.locator(".overlay .object-outline").first());

test.describe("pictures inside containers", () => {
  test("a click on a floating logo in a field result or a content control selects it (HF-166)", async ({
    page,
    consoleErrors,
  }) => {
    const at = await openWrapped(page);
    for (const logo of [FIELD_LOGO, CONTROL_LOGO]) {
      await page.keyboard.press("Escape");
      const point = at(logo);
      await page.mouse.click(point.x, point.y);
      await expect
        .poll(() => objectState(page), { message: "the click selects the picture, not a caret behind it" })
        .toMatchObject({ kind: "image", mode: "selected" });
    }

    // And the selection is a real one: a drag moves the picture and Delete
    // removes it, both undoable.
    const point = at(FIELD_LOGO);
    await page.mouse.click(point.x, point.y);
    const before = await outlineBox(page);
    await page.mouse.move(point.x, point.y);
    await page.mouse.down();
    await page.mouse.move(point.x + 60, point.y + 40, { steps: 8 });
    await page.mouse.up();
    await expect.poll(async () => Math.round((await outlineBox(page)).x - before.x)).toBeGreaterThan(40);
    await page.keyboard.press("Delete");
    await expect.poll(() => objectState(page)).toMatchObject({ kind: null });
    await page.keyboard.press(`${MOD}+z`);
    await page.keyboard.press(`${MOD}+z`);
    await page.keyboard.press("Escape");
    await page.mouse.click(point.x, point.y);
    await expect.poll(() => objectState(page)).toMatchObject({ kind: "image" });
    expect(consoleErrors).toEqual([]);
  });

  test("a picture inside a group takes alt text from its right-click menu (HF-214)", async ({ page, consoleErrors }) => {
    const at = await openWrapped(page);
    const point = at(PARTNER_B);
    await page.mouse.click(point.x, point.y); // the group, as a unit
    await expect.poll(() => objectState(page)).toMatchObject({ kind: "group" });
    await page.mouse.click(point.x, point.y); // then the picture inside it
    await expect.poll(() => objectState(page)).toMatchObject({ kind: "image", path: "1" });

    await page.mouse.click(point.x, point.y, { button: "right" });
    const menu = page.locator(".editor-context-menu");
    await expect(menu.locator('[data-command-id="object.crop"]')).toBeVisible();
    await menu.locator('[data-command-id="object.altText"]').click();
    const input = page.locator("#altTextInput");
    await expect(input).toHaveValue("Partner B");
    await input.fill("The second partner's logo");
    await page.keyboard.press("Enter");
    await expect(page.locator("#altTextDialog")).toBeHidden();

    // Re-open: the dialog reads the model, so this is the write landing.
    await page.mouse.click(point.x, point.y, { button: "right" });
    await menu.locator('[data-command-id="object.altText"]').click();
    await expect(input).toHaveValue("The second partner's logo");
    expect(consoleErrors).toEqual([]);
  });

  test("Delete on a picture inside a group deletes that picture, not the group (HF-214)", async ({
    page,
    consoleErrors,
  }) => {
    const at = await openWrapped(page);
    const pictures = page.locator("#a11yDocument img");
    const point = at(PARTNER_B);
    await page.mouse.click(point.x, point.y);
    await page.mouse.click(point.x, point.y);
    await expect.poll(() => objectState(page)).toMatchObject({ kind: "image", path: "1" });
    await page.keyboard.press("Delete");
    await expect(page.locator("#a11yDocument img[alt='Partner B']")).toHaveCount(0);
    await expect(
      page.locator("#a11yDocument img[alt='Partner A']"),
      "the other logo in the group must survive",
    ).toHaveCount(1);
    await page.keyboard.press(`${MOD}+z`);
    await expect(page.locator("#a11yDocument img[alt='Partner B']")).toHaveCount(1);
    await expect(pictures).toHaveCount(5);
    expect(consoleErrors).toEqual([]);
  });
});

test.describe("Word's Picture Format controls", () => {
  test("Change picture swaps the image and keeps the picture where it was (HF-252)", async ({ page, consoleErrors }) => {
    const at = await openWrapped(page);
    const point = at(FIELD_LOGO);
    await page.mouse.click(point.x, point.y);
    const before = await outlineBox(page);
    const node = (await objectState(page)).node;

    // A SQUARE image into a 2:1 frame: it must be fitted at its own
    // proportions — as tall as the frame and half as wide — not stretched.
    const png = await page.evaluate(async () => {
      const canvas = new OffscreenCanvas(40, 40);
      const g = canvas.getContext("2d");
      g.fillStyle = "#c33";
      g.fillRect(0, 0, 40, 40);
      const blob = await canvas.convertToBlob({ type: "image/png" });
      return [...new Uint8Array(await blob.arrayBuffer())];
    });
    await page.mouse.click(point.x, point.y, { button: "right" });
    const chooser = page.waitForEvent("filechooser");
    await page.locator('.editor-context-menu [data-command-id="object.changePicture"]').click();
    await (await chooser).setFiles({ name: "square.png", mimeType: "image/png", buffer: Buffer.from(png) });

    await expect(page.locator("#status")).toContainText("Picture changed");
    await expect(page.locator("#status")).toContainText("alt text");
    const after = await outlineBox(page);
    expect((await objectState(page)).node, "the same object stays selected").toBe(node);
    expect(Math.abs(after.x - before.x), "it stays where it was").toBeLessThan(2);
    expect(Math.abs(after.height - before.height)).toBeLessThan(2);
    expect(Math.abs(after.width - after.height), "a square image stays square").toBeLessThan(2);

    await page.keyboard.press(`${MOD}+z`);
    await expect.poll(async () => Math.round((await outlineBox(page)).width)).toBe(Math.round(before.width));

    // The chip offers it too (a second surface), as an icon with a name.
    await expect(page.locator('.object-context-bar [aria-label="Change picture"]')).toBeVisible();
    expect(consoleErrors).toEqual([]);
  });

  test("Picture border is on the chip and the menu, and the picture keeps it (HF-254)", async ({ page, consoleErrors }) => {
    const at = await openWrapped(page);
    const point = at(CONTROL_LOGO);
    await page.mouse.click(point.x, point.y);
    await expect.poll(() => objectState(page)).toMatchObject({ kind: "image", outline: "none" });

    await page.locator('.object-context-bar [aria-label="Picture border"]').click();
    const swatch = page.locator("#shapeOutlineMenu [data-shape-color]").first();
    const hex = (await swatch.getAttribute("data-shape-color")).toLowerCase();
    await swatch.click();
    await expect.poll(async () => (await objectState(page)).outline?.toLowerCase()).toBe(hex);

    // The right-click menu says the same word and removes it again.
    await page.mouse.click(point.x, point.y, { button: "right" });
    const menu = page.locator(".editor-context-menu");
    const border = menu.locator('[data-command-id="object.outline"]');
    await expect(border).toContainText("Picture border");
    await border.hover();
    await page.locator('[data-command-id="object.outline.none"]').click();
    await expect.poll(() => objectState(page)).toMatchObject({ outline: "none" });
    expect(consoleErrors).toEqual([]);
  });
});

/** The painted colour at a client point, read from the page's own canvas. */
const paintedAt = (page, point) =>
  page.evaluate(({ x, y }) => {
    const canvas = document.querySelector(".page-wrap .page");
    const rect = canvas.getBoundingClientRect();
    const px = Math.floor(((x - rect.left) * canvas.width) / rect.width);
    const py = Math.floor(((y - rect.top) * canvas.height) / rect.height);
    const [r, g, b] = canvas.getContext("2d").getImageData(px, py, 1, 1).data;
    return { r, g, b };
  }, point);
const isBlue = ({ r, b }) => b > 150 && r < 120;
const isOrange = ({ r, b }) => r > 180 && b < 90;

test.describe("rotate and flip (HF-056)", () => {
  test("Flip and Rotate reach a picture from its chip and its right-click menu, and undo", async ({
    page,
    consoleErrors,
  }) => {
    const at = await openWrapped(page);
    const point = at(CONTROL_LOGO);
    // The fixture's picture is blue on its left half and orange on its right.
    const left = at({ x: CONTROL_LOGO.x - 0.4, y: CONTROL_LOGO.y });
    expect(isBlue(await paintedAt(page, left)), "the fixture paints blue on the left").toBe(true);

    await page.mouse.click(point.x, point.y);
    await page.locator('.object-context-bar [aria-label="Rotate"]').click();
    await page.locator('#objectRotateMenu [data-rotate="flipH"]').click();
    await expect.poll(async () => isOrange(await paintedAt(page, left)), { message: "Flip horizontal mirrored it" }).toBe(true);
    await page.keyboard.press(`${MOD}+z`);
    await expect.poll(async () => isBlue(await paintedAt(page, left))).toBe(true);

    // Rotate right from the right-click menu: the properties panel reads 90.
    await page.mouse.click(point.x, point.y);
    await page.mouse.click(point.x, point.y, { button: "right" });
    await page.locator('.editor-context-menu [data-command-id="object.rotate"]').hover();
    await page.locator('[data-command-id="object.rotate.right90"]').click();
    await page.locator('.object-bar-btn[aria-label="Open object properties"]').click();
    await expect(page.locator(".object-inspector [data-object-prop=rotation]")).toHaveValue("90");
    expect(consoleErrors).toEqual([]);
  });
});

test.describe("the object properties panel", () => {
  test("the panel follows a drag-resize instead of putting the old size back (HF-057)", async ({ page, consoleErrors }) => {
    const at = await openWrapped(page);
    const point = at(FIELD_LOGO);
    await page.mouse.click(point.x, point.y);
    await page.locator('.object-bar-btn[aria-label="Open object properties"]').click();
    const width = page.locator(".object-inspector [data-object-prop=width]");
    await expect(width).toHaveValue("1.5");
    // Drag the south-east grip out by about half an inch.
    const box = await outlineBox(page);
    await page.mouse.move(box.x + box.width, box.y + box.height);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width + 48, box.y + box.height + 24, { steps: 6 });
    await page.mouse.up();
    await expect.poll(async () => Number(await width.inputValue()), { message: "the panel shows the dragged size" }).toBeGreaterThan(1.8);
    // Apply now writes what the panel shows — it used to snap the drag back.
    const dragged = (await outlineBox(page)).width;
    await page.locator(".object-inspector [data-object-inspector-apply]").click();
    await expect.poll(async () => Math.round((await outlineBox(page)).width)).toBe(Math.round(dragged));
    expect(consoleErrors).toEqual([]);
  });

  test("a text box's body settings apply and read back (HF-253)", async ({ page, consoleErrors }) => {
    await openWrapped(page);
    await page.locator('[data-tab="insert"]').click();
    await page.locator("#insertTextBoxBtn").click();
    await page.keyboard.type("Body text");
    await page.keyboard.press("Escape");
    await expect.poll(() => objectState(page)).toMatchObject({ kind: "textbox", mode: "selected" });
    await page.locator('.object-bar-btn[aria-label="Open object properties"]').click();
    const panel = page.locator(".object-inspector");
    // Word's own defaults, read from the model — not zeroes.
    await expect(panel.locator('[data-object-textbox-inset="left"]')).toHaveValue("0.1");
    await expect(panel.locator('[data-object-textbox-inset="top"]')).toHaveValue("0.05");

    const sheet = page.locator(".page-wrap .page").first();
    const before = await sheet.screenshot();
    await panel.locator('[data-object-textbox-inset="left"]').fill("0.6");
    await panel.locator("[data-object-textbox-anchor]").selectOption("bottom");
    await panel.locator("[data-object-inspector-textbox-apply]").click();
    // The text moved on the page…
    await expect.poll(async () => Buffer.compare(await sheet.screenshot(), before), { message: "the page repainted" }).not.toBe(0);
    // …and the panel, re-read from the model, says what was applied.
    await panel.locator(".panel-close").click();
    await page.locator('.object-bar-btn[aria-label="Open object properties"]').click();
    await expect(panel.locator('[data-object-textbox-inset="left"]')).toHaveValue("0.6");
    await expect(panel.locator("[data-object-textbox-anchor]")).toHaveValue("bottom");
    expect(consoleErrors).toEqual([]);
  });
});

test.describe("the keyboard on a selected picture", () => {
  test("Ctrl+Arrow nudges by a pixel and keeps the selection, as in Word", async ({ page, consoleErrors }) => {
    const at = await openWrapped(page);
    const point = at(FIELD_LOGO);
    await page.mouse.click(point.x, point.y);
    const before = await outlineBox(page);
    await page.keyboard.press(`${WORD_MOD}+ArrowRight`);
    await expect.poll(() => objectState(page), { message: "Ctrl+Arrow must not drop the object" }).toMatchObject({
      kind: "image",
      mode: "selected",
    });
    const fine = (await outlineBox(page)).x - before.x;
    expect(fine).toBeGreaterThan(0.5);
    expect(fine).toBeLessThan(2.5);
    await page.keyboard.press("ArrowRight");
    const plain = (await outlineBox(page)).x - before.x - fine;
    expect(plain, "a plain arrow takes the bigger step").toBeGreaterThan(fine);
    expect(consoleErrors).toEqual([]);
  });

  test("in crop mode the arrows move the crop, never the picture (HF-106)", async ({ page, consoleErrors }) => {
    const at = await openWrapped(page);
    const point = at(FIELD_LOGO);
    await page.mouse.click(point.x, point.y);
    const picture = await outlineBox(page);
    await page.locator('.object-context-bar [aria-label="Crop image"]').click();
    await expect(page.locator(".overlay .object-crop-rect")).toBeVisible();

    // Nothing cropped yet: an arrow has nothing to move, and SAYS so — and the
    // one thing it must not do is reach the nudge and move the picture.
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Shift+ArrowRight");
    expect((await outlineBox(page)).x, "an arrow in crop mode moved the PICTURE").toBeCloseTo(picture.x, 0);
    await expect(page.locator("#status")).toContainText("Crop an edge first");

    // Crop the left edge by dragging its grip, then move the kept area right.
    const west = await stableBox(page.locator('.overlay .object-crop-handle[data-handle="7"]'));
    await page.mouse.move(west.x + west.width / 2, west.y + west.height / 2);
    await page.mouse.down();
    await page.mouse.move(west.x + 40, west.y + west.height / 2, { steps: 6 });
    await page.mouse.up();
    const kept = await stableBox(page.locator(".overlay .object-crop-rect"));
    await page.keyboard.press("Shift+ArrowLeft");
    const moved = await stableBox(page.locator(".overlay .object-crop-rect"));
    expect(moved.x, "the kept area moved left").toBeLessThan(kept.x - 2);
    expect(Math.abs(moved.width - kept.width), "at the same size").toBeLessThan(1.5);
    expect((await outlineBox(page)).x, "and the picture stayed put").toBeCloseTo(picture.x, 0);
    await page.keyboard.press("Escape");
    await expect(page.locator(".overlay .object-crop-rect")).toHaveCount(0);
    expect((await outlineBox(page)).x).toBeCloseTo(picture.x, 0);
    expect(consoleErrors).toEqual([]);
  });

  test("dragging an in-line picture that cannot move says why (HF-259)", async ({ page, consoleErrors }) => {
    const at = await openWrapped(page);
    // Reach the in-line picture the way a keyboard user does: select any
    // object, then Tab through the document's objects to it. It is the RESULT
    // of an INCLUDEPICTURE field, so it can be moved neither freely nor in the
    // text on its own — an in-line picture in plain text now moves
    // (`object-text-move.spec.mjs`, UX-OB-02), so this is the one that still
    // has to explain itself.
    const start = at(FIELD_LOGO);
    await page.mouse.click(start.x, start.y);
    let found = false;
    for (let i = 0; i < 8 && !found; i++) {
      await page.keyboard.press("Tab");
      const caps = ((await page.locator("#pages").getAttribute("data-object-capabilities")) ?? "").split(",");
      const kind = await page.locator("#pages").getAttribute("data-object-kind");
      found = kind === "image" && !caps.includes("canMove") && !caps.includes("canMoveInText");
    }
    expect(found, "the fixture's in-line field picture is reachable").toBe(true);
    const box = await outlineBox(page);
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 2 + 30, { steps: 6 });
    await page.mouse.up();
    await expect(page.locator("#status")).toContainText("part of a link, a field, a content control");
    expect(consoleErrors).toEqual([]);
  });
});

test.describe("the chip and the clipboard", () => {
  test("the object chip leaves with its object when the page scrolls (HF-058)", async ({ page, consoleErrors }) => {
    const at = await openWrapped(page);
    const point = at(FIELD_LOGO);
    await page.mouse.click(point.x, point.y);
    const bar = page.locator(".object-context-bar");
    await expect(bar).toBeVisible();
    const top = (await stableBox(bar)).y;
    // Scroll the way a reader does — the wheel — until the picture is gone.
    await page.mouse.move(640, 600);
    for (let i = 0; i < 8; i++) await page.mouse.wheel(0, 250);
    await expect(bar, "the chip must not stay over unrelated text").toBeHidden();
    for (let i = 0; i < 8; i++) await page.mouse.wheel(0, -250);
    await expect(bar).toBeVisible();
    expect(Math.abs((await stableBox(bar)).y - top)).toBeLessThan(3);
    expect(consoleErrors).toEqual([]);
  });

  test("Ctrl+V over a copied image pastes it (HF-059)", async ({ page, context, consoleErrors }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    const at = await openWrapped(page);
    const images = page.locator("#a11yDocument img");
    const before = await images.count();
    // Into body text clear of every logo (and clear of the status bar, which
    // covers the sheet's foot in a 900px window).
    const caret = at({ x: 6.5, y: 3.2 });
    await page.mouse.click(caret.x, caret.y);
    await page.evaluate(async () => {
      const canvas = new OffscreenCanvas(30, 20);
      canvas.getContext("2d").fillRect(0, 0, 30, 20);
      const blob = await canvas.convertToBlob({ type: "image/png" });
      await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
    });
    await page.keyboard.press(`${MOD}+v`);
    await expect(page.locator("#status")).toContainText("Picture inserted");
    await expect(images).toHaveCount(before + 1);
    expect(consoleErrors).toEqual([]);
  });
});

test.describe("a crop gesture the browser cancels", () => {
  test.use({ hasTouch: true });

  test("a touch the browser takes back leaves the crop where it started (HF-106)", async ({ page, consoleErrors }) => {
    const at = await openWrapped(page);
    const point = at(FIELD_LOGO);
    await page.mouse.click(point.x, point.y);
    await page.locator('.object-context-bar [aria-label="Crop image"]').click();
    const kept = page.locator(".overlay .object-crop-rect");
    const start = await stableBox(kept);
    const west = await stableBox(page.locator('.overlay .object-crop-handle[data-handle="7"]'));

    // A real touch, through the browser's own input pipeline: the grip receives
    // a touch `pointerdown`, the finger moves, and then the browser CANCELS the
    // touch — what it does when it decides the gesture was a scroll, or when a
    // pen leaves range. There is no `pointerup`.
    const cdp = await page.context().newCDPSession(page);
    const touch = (type, x, y) =>
      cdp.send("Input.dispatchTouchEvent", { type, touchPoints: type === "touchCancel" ? [] : [{ x, y }] });
    const x0 = west.x + west.width / 2;
    const y0 = west.y + west.height / 2;
    await touch("touchStart", x0, y0);
    await touch("touchMove", x0 + 20, y0);
    await touch("touchMove", x0 + 45, y0);
    await expect
      .poll(async () => (await stableBox(kept)).x - start.x, { message: "the drag was live" })
      .toBeGreaterThan(20);
    await touch("touchCancel", 0, 0);

    // The crop goes back to where the gesture began…
    await expect.poll(async () => Math.round((await stableBox(kept)).x - start.x)).toBe(0);
    // …and stops following the pointer: before the fix it tracked every move
    // until Escape.
    await page.mouse.move(x0 + 80, y0);
    await page.mouse.move(x0 + 120, y0);
    expect(Math.round((await stableBox(kept)).x - start.x)).toBe(0);
    expect(consoleErrors).toEqual([]);
  });
});
