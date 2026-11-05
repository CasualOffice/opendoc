// Editing feel against Word and Google Docs (`docs/109` UX-058, UX-059, UX-060,
// HF-103): Undo re-selects what it restored, a select-all delete removes a table
// instead of leaving its empty grid, a double- or triple-click drag extends by
// whole words or paragraphs, and macOS Option+Up/Down move by paragraph.
//
// Every assertion is made by TYPING over the result and reading the text back:
// what a user's next keystroke does is the guarantee, and it cannot be satisfied
// by a selection that merely looks right.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  runAppMenuCommand,
  stableBox,
  MOD,
  WORD_MOD,
} from "./fixtures.mjs";

const mirror = (page) => page.locator("#a11yDocument");

/** A new blank document holding `text` (`accessibility-checker.spec.mjs`). */
async function blankDocumentWithText(page, text) {
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(
    () => document.getElementById("status")?.textContent.startsWith("Ready — open"),
    null,
    { timeout: 45_000 },
  );
  await runAppMenuCommand(page, "file", "file.new");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
  await clickIntoFirstPage(page);
  await page.locator("#pages").pressSequentially(text);
}

/** The viewport centre of characters [from, from + count) of the caret's line,
 *  measured from the selection highlight the keyboard draws over them. */
async function charsCentre(page, from, count) {
  await page.keyboard.press("Home");
  for (let i = 0; i < from; i += 1) await page.keyboard.press("ArrowRight");
  for (let i = 0; i < count; i += 1) await page.keyboard.press("Shift+ArrowRight");
  const box = await stableBox(page.locator(".page-wrap .overlay .highlight").first());
  await page.keyboard.press("ArrowRight");
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

test.describe("Undo re-selects what it restored (UX-058)", () => {
  async function fresh(page) {
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    await moveCaretToDocStart(page);
    await page.keyboard.press("Enter");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.type("one two three");
  }

  test("after undoing bold, typing replaces the formatted word", async ({ page, consoleErrors }) => {
    await fresh(page);
    await page.keyboard.press(`Shift+${WORD_MOD}+ArrowLeft`); // "three"
    await page.keyboard.press(`${MOD}+b`);
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press(`${MOD}+z`);
    await page.keyboard.type("X");
    // Mutation proof: a collapsed undo caret measured "Xone two three" live.
    await expect(mirror(page).locator("h1").first()).toHaveText("one two X");
    expect(consoleErrors).toEqual([]);
  });

  test("after undoing a delete, typing replaces the restored text", async ({ page, consoleErrors }) => {
    await fresh(page);
    await page.keyboard.press("Home");
    for (let i = 0; i < 4; i += 1) await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Shift+End");
    await page.keyboard.press("Backspace");
    await page.keyboard.press(`${MOD}+z`);
    await page.keyboard.type("X");
    // Mutation proof: measured "one two threeX" with a collapsed undo caret.
    await expect(mirror(page).locator("h1").first()).toHaveText("one X");
    expect(consoleErrors).toEqual([]);
  });
});

test("select-all and Delete removes a table instead of leaving its empty grid (UX-059)", async ({ page, consoleErrors }) => {
  await blankDocumentWithText(page, "Results");
  await page.keyboard.press("Enter");
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(mirror(page).locator("table")).toHaveCount(1);

  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.press(`${MOD}+a`);
  await page.keyboard.press("Backspace");
  // Mutation proof: emptying the cells instead leaves the table in the mirror.
  await expect(mirror(page).locator("table")).toHaveCount(0);
  await expect(page.locator("#status")).not.toHaveText(/isn't supported|not supported/i);
  await page.keyboard.type("fresh");
  await expect(mirror(page)).toContainText("fresh");

  await page.keyboard.press(`${MOD}+z`);
  await page.keyboard.press(`${MOD}+z`);
  await expect(mirror(page).locator("table")).toHaveCount(1);
  expect(consoleErrors).toEqual([]);
});

test.describe("selection by unit (UX-060)", () => {
  test("a double-click drag extends by whole words and keeps the first word", async ({ page, consoleErrors }) => {
    await blankDocumentWithText(page, "quick brown fox jumps");
    const quick = await charsCentre(page, 0, 5);
    const fox = await charsCentre(page, 13, 1); // the middle of "fox"
    await page.mouse.move(quick.x, quick.y);
    await page.mouse.down();
    await page.mouse.up();
    await page.mouse.down({ clickCount: 2 });
    await page.mouse.move(fox.x, fox.y, { steps: 8 });
    await page.mouse.up({ clickCount: 2 });
    await page.keyboard.type("Z");
    // Mutation proof: a character drag, replaced on release by the word under
    // the pointer, leaves "quick brown Z jumps".
    await expect(mirror(page)).toContainText("Z jumps");
    await expect(mirror(page)).not.toContainText("quick");
    expect(consoleErrors).toEqual([]);
  });

  test("Shift+click after a double-click extends to a word end", async ({ page, consoleErrors }) => {
    await blankDocumentWithText(page, "quick brown fox jumps");
    const quick = await charsCentre(page, 0, 5);
    const fox = await charsCentre(page, 13, 1);
    await page.mouse.dblclick(quick.x, quick.y);
    await page.keyboard.down("Shift");
    await page.mouse.click(fox.x, fox.y);
    await page.keyboard.up("Shift");
    await page.keyboard.type("Z");
    // A character extension would leave "Zx jumps".
    await expect(mirror(page)).toContainText("Z jumps");
    await expect(mirror(page)).not.toContainText("x jumps");
    expect(consoleErrors).toEqual([]);
  });

  test("a triple-click drag into the next paragraph selects both whole paragraphs", async ({ page, consoleErrors }) => {
    await blankDocumentWithText(page, "alpha one");
    await page.keyboard.press("Enter");
    await page.locator("#pages").pressSequentially("beta two");
    const beta = await charsCentre(page, 1, 1);
    await page.keyboard.press("ArrowUp");
    const alpha = await charsCentre(page, 6, 1);
    await page.mouse.move(alpha.x, alpha.y);
    await page.mouse.down();
    await page.mouse.up();
    await page.mouse.down({ clickCount: 2 });
    await page.mouse.up({ clickCount: 2 });
    await page.mouse.down({ clickCount: 3 });
    await page.mouse.move(beta.x, beta.y, { steps: 8 });
    await page.mouse.up({ clickCount: 3 });
    await page.keyboard.type("Z");
    // The paragraph under the release point alone would leave "alpha one".
    await expect(mirror(page)).not.toContainText("alpha");
    await expect(mirror(page)).not.toContainText("two");
    await expect(mirror(page)).toContainText("Z");
    expect(consoleErrors).toEqual([]);
  });
});

test("macOS Option+Down moves to the start of the next paragraph (HF-103)", async ({ page, consoleErrors }) => {
  // The keymap is read from the platform at load; claim a Mac so the chord is
  // exercised on every CI runner, not only on a macOS one.
  await page.addInitScript(() => {
    Object.defineProperty(Navigator.prototype, "platform", { get: () => "MacIntel" });
  });
  await blankDocumentWithText(page, "first line here");
  await page.keyboard.press("Enter");
  await page.locator("#pages").pressSequentially("second");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Home");
  for (let i = 0; i < 3; i += 1) await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Alt+ArrowDown");
  await page.keyboard.type("Y");
  // Mutation proof: a dead chord types the Y where the caret was, "firYst".
  await expect(mirror(page)).toContainText("Ysecond");
  await expect(mirror(page)).toContainText("first line here");
  expect(consoleErrors).toEqual([]);
});
