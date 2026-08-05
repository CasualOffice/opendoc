// Format painter (Word/Docs "copy formatting → paint onto target"). A single
// click on the brush captures the caret/selection's formatting and arms a
// one-shot paint; the next document click (expanded to the clicked word) or drag
// receives it, then it disarms. Double-clicking the brush locks it (sticky) so
// successive targets keep receiving the format until Esc. These specs prove the
// captured format is actually applied through the toolbar's own edit ops (a
// prior attempt stalled at "range set but bold not applied").
//
// The demo corpus already contains colored text, so each target is first forced
// to a known state (not bold, explicit blue) — paint (→ red) versus no paint
// (→ stays blue) is then an unambiguous signal independent of demo content.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

const RED = "rgb(255, 0, 0)";
const BLUE = "rgb(0, 0, 255)";

async function selectForward(page, count) {
  for (let i = 0; i < count; i += 1) await page.keyboard.press("Shift+ArrowRight");
}

function barColor(page) {
  return page.locator("#textColorBar").evaluate((el) => getComputedStyle(el).backgroundColor);
}

async function pickTextColor(page, hex) {
  await page.locator("#textColor").click();
  await page.locator("#textColorMenu").locator(`[data-color="${hex}"]`).first().click();
}

// Makes the document's first word bold + red — the reusable "source" formatting
// every spec copies from — and leaves it selected so the brush captures it.
async function selectBoldRedSource(page) {
  await moveCaretToDocStart(page);
  await selectForward(page, 4);
  // Ensure bold is ON regardless of the source's starting state (the demo's
  // first word may already be a bold heading — a blind toggle would clear it).
  if ((await page.locator("#bold").getAttribute("aria-pressed")) !== "true") {
    await page.locator("#bold").click();
  }
  await pickTextColor(page, "#ff0000");
  await moveCaretToDocStart(page);
  await selectForward(page, 4);
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => barColor(page)).toBe(RED);
}

// Picks a word well past the source, forces it to a known state (not bold,
// explicit blue), and returns its viewport centre for later painting.
async function prepareBlueTarget(page) {
  await moveCaretToDocStart(page);
  for (let i = 0; i < 25; i += 1) await page.keyboard.press("ArrowRight");
  await selectForward(page, 4);
  const box = await page.locator(".page-wrap .overlay .highlight").first().boundingBox();
  expect(box).not.toBeNull();
  const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  await page.mouse.dblclick(point.x, point.y); // select the whole word
  if ((await page.locator("#bold").getAttribute("aria-pressed")) === "true") {
    await page.locator("#bold").click();
  }
  await pickTextColor(page, "#0000ff");
  await page.mouse.dblclick(point.x, point.y);
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "false");
  await expect.poll(() => barColor(page)).toBe(BLUE);
  return point;
}

// Selects the word at a viewport point and reports whether it is bold + red.
async function targetState(page, point) {
  await page.mouse.dblclick(point.x, point.y);
  const bold = (await page.locator("#bold").getAttribute("aria-pressed")) === "true";
  const color = await barColor(page);
  return { bold, color };
}

// The viewport centre of the word ~`start` characters into the document, from
// the current (post-reflow) layout — used to locate paint targets while the
// brush is armed (keyboard selection never triggers a paint).
async function wordPointAt(page, start) {
  await moveCaretToDocStart(page);
  for (let i = 0; i < start; i += 1) await page.keyboard.press("ArrowRight");
  await selectForward(page, 4);
  const box = await page.locator(".page-wrap .overlay .highlight").first().boundingBox();
  expect(box).not.toBeNull();
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

// Forces the word ~`start` characters in to a known state (not bold, blue) —
// done before the brush is armed, so paint (→ red + bold) is an unambiguous
// change. Waits for the state to settle so a stale run never leaks into a spec.
async function neutralizeWordAt(page, start) {
  const point = await wordPointAt(page, start);
  await page.mouse.dblclick(point.x, point.y);
  if ((await page.locator("#bold").getAttribute("aria-pressed")) === "true") {
    await page.locator("#bold").click();
  }
  await pickTextColor(page, "#0000ff");
  await page.mouse.dblclick(point.x, point.y);
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "false");
  await expect.poll(() => barColor(page)).toBe(BLUE);
}

test("one-shot: arm from a bold+red run, click a plain word, it becomes bold+red then disarms", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await selectBoldRedSource(page);
  const target = await prepareBlueTarget(page); // starts blue, not bold

  await selectBoldRedSource(page);
  const brush = page.locator("#formatPainter");
  await brush.click();
  await expect(brush).toHaveAttribute("aria-pressed", "true");

  // Paint the target word with a single click, then the brush disarms.
  await page.mouse.click(target.x, target.y);
  await expect(brush).toHaveAttribute("aria-pressed", "false");

  const after = await targetState(page, target);
  expect(after.bold).toBe(true);
  expect(after.color).toBe(RED);

  expect(consoleErrors).toEqual([]);
});

test("sticky: double-click locks the brush and it paints successive targets until Esc", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Two known blue, not-bold targets (neutralized before the brush is armed —
  // a click while armed would paint). Painting each proves it actually changed.
  await neutralizeWordAt(page, 25);
  await neutralizeWordAt(page, 45);

  await selectBoldRedSource(page);
  const brush = page.locator("#formatPainter");
  await brush.dblclick(); // lock sticky mode
  await expect(brush).toHaveAttribute("aria-pressed", "true");
  await expect(brush).toHaveClass(/is-sticky/);

  // Paint the first target (coords derived fresh from the current layout). After
  // the click the painted word is the live selection, so the toolbar reflects it
  // directly — no extra click that the armed brush would treat as a new paint.
  const first = await wordPointAt(page, 25);
  await page.mouse.click(first.x, first.y);
  await expect(brush).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => barColor(page)).toBe(RED);

  // Paint a second target; the brush is still armed. Its coordinate is derived
  // after the first paint so any reflow is already accounted for.
  const second = await wordPointAt(page, 45);
  await page.mouse.click(second.x, second.y);
  await expect(brush).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => barColor(page)).toBe(RED);

  // Escape stops the sticky paint.
  await page.keyboard.press("Escape");
  await expect(brush).toHaveAttribute("aria-pressed", "false");
  await expect(brush).not.toHaveClass(/is-sticky/);

  expect(consoleErrors).toEqual([]);
});

test("Escape cancels an armed brush before it paints anything", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await selectBoldRedSource(page);
  const target = await prepareBlueTarget(page); // blue, not bold

  await selectBoldRedSource(page);
  const brush = page.locator("#formatPainter");
  await brush.click();
  await expect(brush).toHaveAttribute("aria-pressed", "true");

  // Cancel before painting.
  await page.keyboard.press("Escape");
  await expect(brush).toHaveAttribute("aria-pressed", "false");

  // The next click is an ordinary caret placement — the target stays blue.
  await page.mouse.click(target.x, target.y);
  const after = await targetState(page, target);
  expect(after.color).toBe(BLUE);

  expect(consoleErrors).toEqual([]);
});
