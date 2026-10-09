// Dragging an in-line picture to a new place in the text (`docs/109` UX-OB-02),
// made the way a reader makes it: a real pointer, press on the picture, drag,
// release — Ctrl held to copy, Esc to cancel — and Word's F2 from the keyboard.
//
// Each test asserts what the reader can see afterwards, never the mechanism:
// where the picture now is in the words (it sits between "Rich" and
// "Document"), what the accessibility mirror reads in order, that one Undo puts
// it back, and that a Ctrl-drag leaves two. The fixture is the rich corpus
// document, whose second paragraph ends in a small in-line picture:
//
//   Rich Document                      <- heading
//   Paragraph with an image: [img]
//   [nested table] …
import { MOD, expect, gotoEditor, stableBox, test } from "./fixtures.mjs";

/** Word's copy key for a drag: Ctrl, or Option on a Mac. */
const COPY_KEY = process.platform === "darwin" ? "Alt" : "Control";

/** The rich fixture's picture, as a fraction of the first page (the same point
 *  `object-geometry.spec.mjs` selects it by). */
const IMAGE_POS = { fx: 0.32, fy: 0.1 };

/** The accessibility mirror in reading order, up to the fixture's table:
 *  `h1:Rich Document`, `img`, `p:Paragraph with an image:` — where a picture is
 *  in the document, as a screen reader meets it. */
const mirror = (page) =>
  page.locator("#a11yDocument").evaluate((root) => {
    const out = [];
    for (const el of root.children) {
      if (el.tagName === "TABLE") break;
      out.push(el.tagName === "IMG" ? "img" : `${el.tagName.toLowerCase()}:${el.textContent.trim()}`);
    }
    return out;
  });

const BEFORE = ["h1:Rich Document", "p:Paragraph with an image:", "img"];
const AFTER_HEADING = ["h1:Rich Document", "img", "p:Paragraph with an image:"];

/** Opens the fixture and returns the picture's centre, selected. */
async function selectPicture(page) {
  await gotoEditor(page);
  const sheet = await stableBox(page.locator(".page-wrap .page").first());
  const at = { x: sheet.x + sheet.width * IMAGE_POS.fx, y: sheet.y + sheet.height * IMAGE_POS.fy };
  await page.mouse.click(at.x, at.y);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "image");
  const caps = (await page.locator("#pages").getAttribute("data-object-capabilities")).split(",");
  expect(caps, "an in-line picture moves in the text, not freely").toContain("canMoveInText");
  expect(caps).not.toContain("canMove");
  const box = await outline(page);
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

const outline = (page) => stableBox(page.locator(".overlay .object-outline").first());

/** Where `word` is on screen: Find selects it, and the selection's highlight is
 *  the engine's own geometry for those characters. */
async function wordBox(page, word) {
  // A held object paints its outline instead of a text highlight; let go of it.
  if (await page.locator("#pages").getAttribute("data-object-mode")) await page.keyboard.press("Escape");
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(word);
  await expect(page.locator("#findStatus")).toHaveText("1 match");
  await page.keyboard.press("Escape");
  return stableBox(page.locator(".overlay .highlight").first());
}

/** A real pointer drag from `from` to `to`, optionally holding the copy key
 *  from the middle of the drag to the release, as a hand does. */
async function drag(page, from, to, { copy = false, beforeRelease = null } = {}) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 30, from.y + 10, { steps: 4 });
  if (copy) await page.keyboard.down(COPY_KEY);
  await page.mouse.move(to.x, to.y, { steps: 10 });
  if (beforeRelease) await beforeRelease();
  await page.mouse.up();
  if (copy) await page.keyboard.up(COPY_KEY);
}

test("a dragged in-line picture lands between the words it was dropped between, and one Undo puts it back", async ({
  page,
  consoleErrors,
}) => {
  const picture = await selectPicture(page);
  await expect.poll(() => mirror(page)).toEqual(BEFORE);
  const original = await outline(page);
  const document = await wordBox(page, "Document");
  // Re-select: Find moved the selection to the word.
  await page.mouse.click(picture.x, picture.y);
  // The drop point: just inside the left edge of "Document", so the caret the
  // click hit test answers is the one between "Rich " and "Document".
  const target = { x: document.x + 2, y: document.y + document.height / 2 };
  await drag(page, picture, target, {
    beforeRelease: async () => {
      // A drop caret follows the pointer, at the place the release will use.
      const caret = await stableBox(page.locator(".overlay .object-drop-caret"));
      expect(Math.abs(caret.x - document.x)).toBeLessThan(6);
    },
  });

  await expect.poll(() => mirror(page), { message: "the picture now follows the heading" }).toEqual(AFTER_HEADING);
  await expect(page.locator(".overlay .object-drop-caret")).toHaveCount(0);
  // Still selected, at its new place: the same picture, held.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "image");
  const moved = await outline(page);
  const rich = await wordBox(page, "Rich");
  const after = await wordBox(page, "Document");
  expect(moved.x, "after the word Rich").toBeGreaterThanOrEqual(rich.x + rich.width - 1);
  // …and before "Document", which it pushed along by its own width. (The
  // picture is zero-width in offset space, so a highlight of "Document" may
  // START at the picture; where the word ENDS is unambiguous.)
  expect(after.x + after.width, "and before the word Document").toBeGreaterThanOrEqual(
    document.x + document.width + moved.width - 2,
  );
  expect(moved.y + moved.height, "on the heading's line").toBeGreaterThan(after.y);
  expect(moved.y).toBeLessThan(after.y + after.height);
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");

  // ONE undo step restores the whole move.
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(() => mirror(page), { message: "one undo puts it back" }).toEqual(BEFORE);
  await page.mouse.click(picture.x, picture.y);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "image");
  const restored = await outline(page);
  expect(Math.abs(restored.x - original.x)).toBeLessThan(2);
  expect(Math.abs(restored.y - original.y)).toBeLessThan(2);
  expect(consoleErrors).toEqual([]);
});

test("a Ctrl-drag leaves the picture where it was and drops a copy, which is selected", async ({
  page,
  consoleErrors,
}) => {
  const picture = await selectPicture(page);
  const originalNode = await page.locator("#pages").getAttribute("data-object-selected");
  const document = await wordBox(page, "Document");
  await page.mouse.click(picture.x, picture.y);
  await drag(page, picture, { x: document.x + 2, y: document.y + document.height / 2 }, { copy: true });

  await expect
    .poll(() => mirror(page), { message: "two pictures: the copy after the heading, the original where it was" })
    .toEqual(["h1:Rich Document", "img", "p:Paragraph with an image:", "img"]);
  const selected = await page.locator("#pages").getAttribute("data-object-selected");
  expect(selected, "the copy is the selection").not.toBe(originalNode);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "image");

  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(() => mirror(page), { message: "one undo removes the copy and only the copy" }).toEqual(BEFORE);
  expect(consoleErrors).toEqual([]);
});

test("Esc cancels the drag, and a picture dropped back where it came from does not change the document", async ({
  page,
  consoleErrors,
}) => {
  const picture = await selectPicture(page);
  const document = await wordBox(page, "Document");
  await page.mouse.click(picture.x, picture.y);
  await drag(page, picture, { x: document.x + 2, y: document.y + document.height / 2 }, {
    beforeRelease: () => page.keyboard.press("Escape"),
  });
  await expect(page.locator(".overlay .object-drop-caret")).toHaveCount(0);
  // A drop back on its own place: away past the threshold, and back.
  await page.mouse.click(picture.x, picture.y);
  await page.mouse.move(picture.x, picture.y);
  await page.mouse.down();
  await page.mouse.move(picture.x + 60, picture.y + 40, { steps: 6 });
  await page.mouse.move(picture.x, picture.y, { steps: 6 });
  await page.mouse.up();

  await expect.poll(() => mirror(page)).toEqual(BEFORE);
  await expect(page.locator("#documentState"), "neither gesture edited anything").toHaveAttribute(
    "data-state",
    "opened",
  );
  await expect(page.locator("#undoBtn")).toBeDisabled();
  expect(consoleErrors).toEqual([]);
});

test("a picture dropped into a table cell is picked up there, resized there, and dragged back out (UX-OB-03)", async ({
  page,
  consoleErrors,
}) => {
  const picture = await selectPicture(page);
  // The fixture's nested table: drop the picture just after "Nested A".
  const cellWord = await wordBox(page, "Nested A");
  await page.mouse.click(picture.x, picture.y);
  await drag(page, picture, { x: cellWord.x + cellWord.width + 1, y: cellWord.y + cellWord.height / 2 });
  await expect.poll(() => mirror(page), { message: "the picture left the body paragraph" }).toEqual([
    "h1:Rich Document",
    "p:Paragraph with an image:",
  ]);

  // Still held, at its new place INSIDE the cell — a picture in a cell had no
  // object box at all, so the selection used to vanish here.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "image");
  const inCell = await outline(page);
  // After "Nested A" in reading order: beside it on its line, or — the cell is
  // narrow — at the start of the next line of the same cell.
  const besideIt = inCell.x >= cellWord.x + cellWord.width - 2 && inCell.y < cellWord.y + cellWord.height;
  const belowIt = inCell.y >= cellWord.y + cellWord.height - 2 && Math.abs(inCell.x - cellWord.x) < 4;
  expect(besideIt || belowIt, `after the words in the cell: ${JSON.stringify({ inCell, cellWord })}`).toBe(true);

  // Let go of it, and pick it up again with a click where it is drawn.
  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-kind", "image");
  const centre = { x: inCell.x + inCell.width / 2, y: inCell.y + inCell.height / 2 };
  await page.mouse.click(centre.x, centre.y);
  await expect(page.locator("#pages"), "a click on a picture in a cell selects it").toHaveAttribute(
    "data-object-kind",
    "image",
  );

  // Resized, in the cell, through its properties panel — the engine's resize
  // starts from the object's placed box, which is exactly what a picture in a
  // cell did not have. (The panel rather than a grip drag: grip drags on this
  // 16px picture are the contention-flaky gesture `object-geometry.spec.mjs`
  // already covers.)
  await page.locator('.object-bar-btn[aria-label="Open object properties"]').click();
  const width = page.locator(".object-inspector [data-object-prop=width]");
  await width.fill("0.5");
  await page.locator(".object-inspector [data-object-inspector-apply]").click();
  await expect.poll(async () => (await outline(page)).width, { message: "it grew" }).toBeGreaterThan(inCell.width + 10);
  await page.locator(".object-inspector .panel-close").click();
  await expect(page.locator(".object-inspector")).toBeHidden();

  // And dragged back out of the cell, into the heading. (A click on it first:
  // the panel's close button had the keyboard focus, not the document.)
  const grown = await outline(page);
  const grownAt = { x: grown.x + grown.width / 2, y: grown.y + grown.height / 2 };
  await page.mouse.click(grownAt.x, grownAt.y);
  const heading = await wordBox(page, "Document");
  await page.mouse.click(grownAt.x, grownAt.y);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "image");
  await drag(page, grownAt, { x: heading.x + 2, y: heading.y + heading.height / 2 });
  await expect.poll(() => mirror(page), { message: "out of the cell, after the heading" }).toEqual([
    "h1:Rich Document",
    "img",
    "p:Paragraph with an image:",
  ]);
  expect(consoleErrors).toEqual([]);
});

test("F2 asks Move to where?, and Enter moves the picture to the insertion point (Word's keyboard move)", async ({
  page,
  consoleErrors,
}) => {
  await selectPicture(page);
  // The second surface is on the right-click menu too, with the key named.
  const centre = await outline(page);
  await page.mouse.click(centre.x + centre.width / 2, centre.y + centre.height / 2, { button: "right" });
  const row = page.locator('.editor-context-menu [data-command-id="object.moveInText"]');
  await expect(row).toBeVisible();
  await row.click();
  await expect(page.locator("#status")).toContainText("Move to where?");
  // The insertion point goes where the picture should, by keyboard alone...
  await page.keyboard.press(`${MOD}+Home`);
  // ...and Enter moves it there, rather than breaking the paragraph.
  await page.keyboard.press("Enter");
  await expect.poll(() => mirror(page), { message: "the picture opens the heading now" }).toEqual(AFTER_HEADING);
  await expect(page.locator("#a11yDocument > h1")).toHaveText("Rich Document");
  await expect(page.locator("#pages"), "and it is selected at its new place").toHaveAttribute(
    "data-object-kind",
    "image",
  );

  // F2 itself, and Esc: nothing moves.
  await page.keyboard.press("F2");
  await expect(page.locator("#status")).toContainText("Move to where?");
  await page.keyboard.press("Escape");
  await expect(page.locator("#status")).toContainText("Move cancelled");
  await expect.poll(() => mirror(page)).toEqual(AFTER_HEADING);
  expect(consoleErrors).toEqual([]);
});
