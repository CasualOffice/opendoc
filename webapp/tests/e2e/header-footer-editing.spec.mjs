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
import { test, expect, gotoEditor, clickIntoFirstPage, setReviewMode, MOD } from "./fixtures.mjs";

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

test("a document with no header gets one created on demand", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page); // the rich fixture has no running content
  await clickIntoFirstPage(page);
  const bodyBefore = await page.locator("#a11yDocument").textContent();

  await runCommand(page, "Edit header");

  // Word and Docs both create the header the moment you ask to edit a document
  // that has none — the ask IS the intent — rather than refusing.
  await expect.poll(() => band(page)).toBe("header");
  await page.keyboard.type("MADE");

  // It is a real header: the text is not in the body projection.
  await expect(page.locator("#undoBtn")).toBeEnabled();
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

  expect(consoleErrors).toEqual([]);
});

test("creating a header is one undoable action, and refused in Viewing", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Viewing is read-only, and adding running content has no tracked-change
  // representation, so it must fail closed rather than mutate.
  await setReviewMode(page, "viewing");
  await runCommand(page, "Edit header");
  await expect(page.locator("#status")).toContainText("read-only");
  expect(await band(page)).toBeNull();
  await expect(page.locator("#undoBtn")).toBeDisabled();

  // Back in Editing it creates, and one undo removes the whole thing — the body
  // and the section's link to it are a single action.
  await setReviewMode(page, "editing");
  await runCommand(page, "Edit header");
  await expect.poll(() => band(page)).toBe("header");
  await expect(page.locator("#undoBtn")).toBeEnabled();
  await page.keyboard.press(`${MOD}+z`);
  await expect(page.locator("#undoBtn")).toBeDisabled();

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

// ---- Regressions from real use ------------------------------------------------
// Everything below was reported by using the editor, after a version shipped that
// passed its own tests. Those tests asserted `data-running-edit` and never looked
// at what the gestures actually did, so they were green while double-clicking the
// header selected a word in the BODY and header text could not be selected at all.

async function pageBox(page) {
  return page.locator(".page-wrap .page").first().boundingBox();
}

test("double-clicking the header band does not select a word in the body", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const box = await pageBox(page);

  // The fixture has no header, so there is nothing in the band to hit-test.
  // Keying the gesture off a hit made it fall through to word-selection, which
  // grabbed a word out of the body — the opposite of what the gesture asks for.
  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 12);

  await expect.poll(() => band(page)).toBe("header");
  // No body word got selected on the way: the floating format toolbar that a
  // word selection raises must not be showing.
  await expect(page.locator("#selToolbar")).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

test("header text can be selected by dragging inside the band", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const box = await pageBox(page);
  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 12);
  await expect.poll(() => band(page)).toBe("header");
  await page.keyboard.type("Quarterly Report 2026");

  // Every click and drag used to resolve through the body walk, so a drag in the
  // header moved the caret into the body and selected nothing there.
  await page.mouse.move(box.x + 80, box.y + 12);
  await page.mouse.down();
  await page.mouse.move(box.x + 240, box.y + 12, { steps: 12 });
  await page.mouse.up();

  expect(await band(page)).toBe("header");
  await expect.poll(() => page.locator(".overlay .highlight").count()).toBeGreaterThan(0);

  expect(consoleErrors).toEqual([]);
});

test("the page is not dimmed while editing running content", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const box = await pageBox(page);
  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 12);
  await expect.poll(() => band(page)).toBe("header");

  // The header is painted INTO the page raster, so dimming the page washed out
  // the very content being edited and greyed the whole sheet.
  const opacity = await page
    .locator(".page-wrap .page")
    .first()
    .evaluate((el) => getComputedStyle(el).opacity);
  expect(Number(opacity)).toBe(1);

  // The band is marked instead — a boundary and a label, as Word and LibreOffice
  // show it.
  await expect(page.locator(".running-band").first()).toBeVisible();
  await expect(page.locator(".running-band-label").first()).toHaveText("Header");

  expect(consoleErrors).toEqual([]);
});

test("clicking the body leaves the header context", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const box = await pageBox(page);
  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 12);
  await expect.poll(() => band(page)).toBe("header");

  // Word, Docs, OnlyOffice and LibreOffice all leave on a click in the body.
  await page.mouse.click(box.x + box.width * 0.3, box.y + box.height * 0.4);
  await expect.poll(() => band(page)).toBeNull();
  await page.keyboard.type("BODYTEXT");
  await expect(page.locator("#a11yDocument")).toContainText("BODYTEXT");

  expect(consoleErrors).toEqual([]);
});

// The toolbar must report the paragraph the caret is actually in. A RIGHT-aligned
// header paragraph reported itself as left-aligned, because every property read
// walked the body alone, found nothing, and the caller fell back to its default.
// A wrong answer is worse than none: it invites "fixing" an alignment that was
// never wrong.
test("the toolbar reflects the header paragraph's own alignment", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  // sample.docx's header is right-aligned; the demo fixture has no header.
  await page.locator("#file").setInputFiles("sample.docx");
  await expect
    .poll(() => page.locator("#docTitle").inputValue(), { timeout: 30_000 })
    .toContain("sample");
  await expect(page.locator(".page-wrap").first()).toBeVisible();

  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await page.mouse.dblclick(box.x + box.width * 0.72, box.y + 45);
  await expect.poll(() => band(page)).toBe("header");

  await expect(page.locator("#alignEnd")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#alignStart")).toHaveAttribute("aria-pressed", "false");

  expect(consoleErrors).toEqual([]);
});
