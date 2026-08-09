// Can a character format be taken OFF again — everywhere, and both at a
// selection and at a bare caret?
//
// Reported as "when i do bold, cant remove that property.. same with italic,
// underline". A selection in the body round-trips, so the defect is somewhere
// else in the space: the caret (armed formatting), or a surface other than the
// body. This drives every combination instead of guessing which one.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  MOD,
} from "./fixtures.mjs";

const MARKS = [
  ["bold", "b"],
  ["italic", "i"],
  ["underline", "u"],
];

/** Moves the caret into ordinary body prose, past the (bold) opening heading. */
async function intoBodyProse(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  for (let i = 0; i < 4; i += 1) await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Home");
}

/** Enters the first page's header and leaves the caret in it. */
async function intoHeader(page) {
  const canvas = page.locator(".page-wrap .page").first();
  await expect(canvas).toBeVisible();
  let box = null;
  await expect
    .poll(async () => {
      box = await canvas.boundingBox();
      return box?.width ?? 0;
    })
    .toBeGreaterThan(0);
  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 12);
  await expect.poll(() => page.locator("#pages").getAttribute("data-running-edit")).toBe("header");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
}

/** Enters the first page's footer. */
async function intoFooter(page) {
  const wrap = page.locator(".page-wrap").first();
  await wrap.evaluate((el) => el.scrollIntoView({ block: "end" }));
  await page.waitForTimeout(300);
  let box = null;
  await expect
    .poll(async () => {
      box = await page.locator(".page-wrap .page").first().boundingBox();
      return box?.width ?? 0;
    })
    .toBeGreaterThan(0);
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await page.mouse.dblclick(box.x + box.width * 0.5, box.y + box.height - 14);
    if ((await page.locator("#pages").getAttribute("data-running-edit")) === "footer") break;
    await page.waitForTimeout(200);
  }
  await expect.poll(() => page.locator("#pages").getAttribute("data-running-edit")).toBe("footer");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
}

/** Inserts a footnote and leaves the caret in its body. */
async function intoFootnote(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Footnote");
  await page.locator("#cmdList .cmd-item", { hasText: "Footnote" }).first().click();
  await expect(page.locator("#status")).toContainText("Footnote added");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
}

/** Inserts a text box and leaves the caret inside it. */
async function intoTextBox(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  // Through the palette, not the Insert ribbon tab: switching tabs is its own
  // question (see `ribbon-focus-audit`), and mixing it in here would test that
  // instead of the toggles.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Text box");
  await page.locator("#cmdList .cmd-item", { hasText: "Text box" }).first().click();
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
}

const SURFACES = [
  ["body", intoBodyProse],
  ["header", intoHeader],
  ["footer", intoFooter],
  ["footnote", intoFootnote],
  ["text box", intoTextBox],
];

for (const [surface, enter] of SURFACES) {
  for (const [mark, key] of MARKS) {
    test(`[${surface}] ${mark} comes back off over a selection`, async ({
      page,
      consoleErrors,
    }) => {
      await gotoEditor(page);
      await enter(page);
      await page.keyboard.type("TOGGLE");
      for (let i = 0; i < 6; i += 1) await page.keyboard.press("Shift+ArrowLeft");

      const button = page.locator(`#${mark}`);
      const initial = await button.getAttribute("aria-pressed");
      const flipped = initial === "true" ? "false" : "true";
      await page.keyboard.press(`${MOD}+${key}`);
      await expect(button).toHaveAttribute("aria-pressed", flipped);
      await page.keyboard.press(`${MOD}+${key}`);
      await expect(button, "the second press must undo the first").toHaveAttribute(
        "aria-pressed",
        initial,
      );

      expect(consoleErrors).toEqual([]);
    });

    test(`[${surface}] ${mark} armed at a caret can be disarmed`, async ({
      page,
      consoleErrors,
    }) => {
      // No selection: the toggle ARMS the format for what you type next. Arming
      // it and changing your mind has to work, and the text typed after must
      // carry the state the button shows.
      await gotoEditor(page);
      await enter(page);

      const button = page.locator(`#${mark}`);
      const initial = await button.getAttribute("aria-pressed");
      const flipped = initial === "true" ? "false" : "true";

      await page.keyboard.press(`${MOD}+${key}`);
      await expect(button).toHaveAttribute("aria-pressed", flipped);
      await page.keyboard.press(`${MOD}+${key}`);
      await expect(button, "an armed format must be disarmable").toHaveAttribute(
        "aria-pressed",
        initial,
      );

      // And the disarm is real, not just the button: what gets typed carries the
      // original state, not the armed one.
      await page.keyboard.type("AFTER");
      for (let i = 0; i < 5; i += 1) await page.keyboard.press("Shift+ArrowLeft");
      await expect(button, "typed text must match the disarmed state").toHaveAttribute(
        "aria-pressed",
        initial,
      );

      expect(consoleErrors).toEqual([]);
    });
  }
}

test("[body] bold removes on text that arrived bold from the file", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Home");
  for (let i = 0; i < 5; i += 1) await page.keyboard.press("Shift+ArrowRight");
  const bold = page.locator("#bold");
  test.skip((await bold.getAttribute("aria-pressed")) !== "true", "opening run is not bold");

  await page.keyboard.press(`${MOD}+b`);
  await expect(bold).toHaveAttribute("aria-pressed", "false");
  expect(consoleErrors).toEqual([]);
});

test("[body] a mixed selection turns fully ON, then fully OFF", async ({
  page,
  consoleErrors,
}) => {
  // Word's rule: a selection that is partly bold goes fully bold on the first
  // press, and fully unbold on the second. A toggle that reads "mixed" and then
  // flips per-run instead can never reach OFF — the shape of "I can't remove it".
  await gotoEditor(page);
  await intoBodyProse(page);
  await page.keyboard.type("AB");
  // Bold only the FIRST character. Typing after a bold run inherits bold, so
  // building the mixed state by typing more text would produce a uniformly bold
  // selection — and then a first press that correctly turns everything OFF
  // would look like the bug this test is hunting.
  await page.keyboard.press("Home");
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press(`${MOD}+b`);
  await page.keyboard.press("Home");
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Shift+ArrowRight");
  // ARIA's tri-state: a partly-bold selection is neither pressed nor unpressed.
  await expect(page.locator("#bold"), "the selection is genuinely mixed").toHaveAttribute(
    "aria-pressed",
    "mixed",
  );

  const bold = page.locator("#bold");
  await page.keyboard.press(`${MOD}+b`);
  await expect(bold, "a mixed selection goes fully on").toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press(`${MOD}+b`);
  await expect(bold, "and then fully off").toHaveAttribute("aria-pressed", "false");

  expect(consoleErrors).toEqual([]);
});
