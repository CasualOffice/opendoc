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
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  setReviewMode,
  stableBox,
  MOD,
} from "./fixtures.mjs";

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
  // `data-running-edit` alone is not the guarantee. This test asserted only the
  // attribute and stayed green through a version where the context opened with no
  // caret at all and the header could not be typed into — the blank-band
  // regressions at the end of this file are what that missing assertion cost.
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

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
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  // sample.docx's DEFAULT header is right-aligned; the demo fixture has no header.
  await page.locator("#file").setInputFiles("sample.docx");
  await expect
    .poll(() => page.locator("#docTitle").inputValue(), { timeout: 30_000 })
    .toContain("sample");
  await expect(page.locator(".page-wrap").first()).toBeVisible();

  // Page TWO, because page one does not show that paragraph: sample.docx sets
  // `w:titlePg` and declares no `first` reference, so page 1 correctly paints a
  // blank band and entering it creates a fresh, unformatted paragraph. Aimed at
  // page 1 this test read `right` from the `default` paragraph it could not see —
  // passing for the wrong reason, off the very body-only property read it was
  // written to catch.
  await dblclickBand(page, 1, "header");
  await expect.poll(() => band(page)).toBe("header");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  await expect(page.locator("#alignEnd")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#alignStart")).toHaveAttribute("aria-pressed", "false");

  expect(consoleErrors).toEqual([]);
});

// ---- A band the page legitimately paints BLANK ---------------------------------
//
// `sample.docx` is the ordinary shape of the near-universal "no header on page 1"
// document: `w:titlePg` is set and the only header/footer references are of type
// `default`. So page 1 correctly paints an EMPTY band — ECMA-376 §17.10.6 gives a
// `w:titlePg` section with no `first` reference "a new blank header" rather than
// falling back to the odd-page one.
//
// The editor could not put a caret in such a band. It opened the context, set
// `data-running-edit`, and resolved the caret from the `default` body the page does
// not show; that position has no geometry on page 1, so nothing painted and the
// user could not type. Entering a blank band must CREATE the variant the page
// shows, which is what Word does and what the spec wording describes.

/** Enters one page's band by double-clicking in it, leaving the assertions to the
 *  caller — the point of these tests is what the user can then DO.
 *
 *  Positioned the way `surface-editing-matrix` positions it: a page is taller than
 *  the viewport, so the band being aimed at has to be scrolled to its own edge
 *  (`block: "start"` / `"end"`) before its box is read, and the gesture is retried
 *  because under load the double-click can land before the scroll settles. A
 *  genuinely dead band fails all three attempts. */
async function dblclickBand(page, index, region) {
  const canvas = page.locator(".page-wrap").nth(index).locator(".page");
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await canvas.evaluate(
      (el, edge) => el.scrollIntoView({ block: edge }),
      region === "header" ? "start" : "end",
    );
    await page.waitForTimeout(150);
    const box = await stableBox(canvas);
    await page.mouse.dblclick(
      box.x + box.width * 0.5,
      region === "header" ? box.y + 12 : box.y + box.height - 12,
    );
    if ((await band(page)) === region) return;
    await page.waitForTimeout(200);
  }
}

for (const region of ["header", "footer"]) {
  test(`a blank first-page ${region} band gets a caret and accepts typing`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await gotoEditor(page);
    await openSample(page);
    const bodyBefore = await page.locator("#a11yDocument").textContent();

    await dblclickBand(page, 0, region);
    await expect.poll(() => band(page)).toBe(region);

    // THE GUARANTEE: a caret the user can see and type at. Not an attribute.
    await expect(page.locator(".overlay .caret")).toHaveCount(1);
    await page.keyboard.type("PAGEONE");
    await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
    // It went into the running content, not the body.
    expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

    // Creating the variant and typing into it are two undoable steps, and the
    // document comes all the way back.
    await page.keyboard.press(`${MOD}+z`);
    await page.keyboard.press(`${MOD}+z`);
    await expect(page.locator("#undoBtn")).toBeDisabled();

    expect(consoleErrors).toEqual([]);
  });
}

test("a blank even-page band gets a caret and accepts typing", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await openSample(page);
  const bodyBefore = await page.locator("#a11yDocument").textContent();

  // Turn on Word's "Different Odd & Even Pages". The document declares no `even`
  // reference, so every even page now paints a blank band — the same situation as
  // page 1 above, reached by a different rule on a different page. Nothing in the
  // fix is special-cased to page 1.
  await clickIntoFirstPage(page);
  await runCommand(page, "Different odd & even pages");
  await expect(page.locator("#status")).toContainText("Different odd & even pages on");

  await dblclickBand(page, 1, "header");
  await expect.poll(() => band(page)).toBe("header");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  await page.keyboard.type("EVENHDR");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

  expect(consoleErrors).toEqual([]);
});
