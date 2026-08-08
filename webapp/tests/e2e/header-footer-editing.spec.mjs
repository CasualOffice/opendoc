// Editing a page header or footer.
//
// Every reference editor treats this as an editing CONTEXT SWITCH — enter the
// band, edit in place with the body de-emphasised, leave with Esc or a click in
// the body — never a modal dialog and never a document mutation in itself
// (docs/85 §10.2). This is the host half; the engine halves landed first: the
// edit ops resolve a position in whichever surface owns it, and a point in a
// running band resolves back to a model position.
//
// There is no sub-document address anywhere in this: a header position is an
// ordinary NodeId + offset, so typing into a header is ordinary text editing.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

// The rich demo fixture has no running content; sample.docx does, so the tests
// that need a real header open it through the ordinary file path.
async function openSample(page) {
  await page.locator("#file").setInputFiles("sample.docx");
  await expect
    .poll(() => page.locator("#docTitle").inputValue(), { timeout: 30_000 })
    .toContain("sample");
  // Let the open settle before driving the keyboard at it.
  await expect(page.locator(".page-wrap").first()).toBeVisible();
}

async function runCommand(page, label) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill(label);
  await page.locator("#cmdList .cmd-item", { hasText: label }).first().click();
  await expect(page.locator("#cmdPalette")).toBeHidden();
}

const band = (page) => page.locator("#pages").getAttribute("data-running-edit");

test("Edit header enters the header context and Esc leaves it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await openSample(page);

  await runCommand(page, "Edit header");
  expect(await band(page)).toBe("header");
  await expect(page.locator("#status")).toContainText("Editing the header");
  // The body is de-emphasised so it is obvious which layer the keystrokes go to.
  await expect(page.locator("body")).toHaveClass(/running-edit/);

  await page.keyboard.press("Escape");
  expect(await band(page)).toBeNull();
  await expect(page.locator("body")).not.toHaveClass(/running-edit/);

  expect(consoleErrors).toEqual([]);
});

test("typing in the header edits the header, not the body", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await openSample(page);
  const bodyTextBefore = await page.locator("#a11yDocument").textContent();

  await runCommand(page, "Edit header");
  await page.keyboard.type("ZZTOP");

  // It is a real, undoable edit on the ordinary typing path.
  await expect(page.locator("#undoBtn")).toBeEnabled();
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  // And it went into the header: the body projection is untouched.
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyTextBefore);
  expect(await page.locator("#a11yDocument").textContent()).not.toContain("ZZTOP");
  // The context is still open, as it is in Word until you leave it.
  expect(await band(page)).toBe("header");

  expect(consoleErrors).toEqual([]);
});

test("double-clicking the header band enters the context", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await openSample(page);

  // The gesture every reference editor uses: double-click in the band itself.
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.dblclick({ position: { x: box.width * 0.5, y: box.height * 0.045 } });

  expect(await band(page)).toBe("header");

  expect(consoleErrors).toEqual([]);
});

test("a document with no header says so instead of opening an empty context", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page); // the rich fixture has no running content
  await clickIntoFirstPage(page);

  await runCommand(page, "Edit header");

  // Creating one on demand is the `+` marker's job and a separate slice; an
  // empty context that swallows keystrokes would be worse than saying no.
  await expect(page.locator("#status")).toContainText("no header yet");
  expect(await band(page)).toBeNull();

  expect(consoleErrors).toEqual([]);
});

test("clicking body text still puts the caret in the body", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await openSample(page);

  // Entering a header must stay a deliberate gesture: an ordinary click, even
  // one near the top of the page, belongs to the body.
  await clickIntoFirstPage(page);
  expect(await band(page)).toBeNull();

  await page.keyboard.type("BODY");
  await expect(page.locator("#a11yDocument")).toContainText("BODY");

  expect(consoleErrors).toEqual([]);
});
