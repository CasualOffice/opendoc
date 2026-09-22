// Ticking a checkbox in a form.
//
// `docs/118` §2. The owner's Medical Incident Report Form carries eight
// `w14:checkbox` content controls. Measured against the real file: click,
// Space and double-click were all no-ops, so the document opened, rendered
// correctly, and was read-only in the one way that matters — it is a form, and
// the form could not be completed.
//
// The control was fully modelled the whole time (`SdtCheckbox` with its
// checked flag and its declared checked/unchecked glyphs). What did not exist
// was the operation and the gesture.
//
// Neither is invented here. ONLYOFFICE's `CInlineLevelSdt.ToggleCheckBox`
// flips the flag and rewrites the control's CONTENT to the declared symbol;
// Word toggles on a click and on Space, as one undo step. Both agree, so this
// asserts what they agree on.
import {
  test,
  expect,
  gotoEditor,
  setReviewMode,
  stableBox,
} from "./fixtures.mjs";

const FORM = "../fixtures/generated/form-checkbox.docx";

/** How many of each box the document's text currently holds.
 *
 *  Counted at the PRIVATE-USE code points, `F0A3`/`F052`, because that is how
 *  Word writes a symbol-font glyph in content and therefore what the document
 *  actually carries — measured on the owner's own form before any of this was
 *  built. Reading the glyph rather than an internal flag is the point: the
 *  defect being guarded is a flag that moves while the box a person sees does
 *  not. */
async function boxes(page) {
  return page.locator("#a11yDocument").evaluate((el) => {
    const text = el.innerText;
    return {
      unchecked: [...text].filter((c) => c.codePointAt(0) === 0xf0a3).length,
      checked: [...text].filter((c) => c.codePointAt(0) === 0xf052).length,
    };
  });
}

async function openForm(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(FORM);
  await expect(page.locator("#a11yDocument")).toContainText("Tick one");
}

/** Puts the caret at the start of the control's paragraph, using the keyboard
 *  so no pixel is guessed, and returns the on-screen point of the glyph — read
 *  from the caret the editor itself draws there. */
async function focusCheckbox(page) {
  const mod = process.platform === "darwin" ? "Meta" : "Control";
  await page.locator("#pages").click({ position: { x: 300, y: 40 } });
  await page.keyboard.press(`${mod}+Home`);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Home");
  const caret = page.locator(".overlay .caret").first();
  await expect(caret).toBeVisible();
  const box = await stableBox(caret);
  // Just inside the glyph, which begins where the caret sits.
  return { x: box.x + 3, y: box.y + box.height / 2 };
}

test("clicking a form checkbox ticks it, and one undo puts it back", async ({
  page,
  consoleErrors,
}) => {
  await openForm(page);
  const before = await boxes(page);
  expect(
    before.unchecked,
    "the fixture must start with an empty box",
  ).toBeGreaterThan(0);

  const at = await focusCheckbox(page);
  await page.mouse.click(at.x, at.y);
  const ticked = await boxes(page);
  expect(ticked.checked, "the glyph must become the ticked one").toBe(
    before.checked + 1,
  );
  expect(ticked.unchecked).toBe(before.unchecked - 1);

  // One undo step, as in Word and as in ONLYOFFICE's
  // `CChangesSdtPrCheckBoxChecked`.
  await page.locator("#undoBtn").click();
  await expect.poll(() => boxes(page)).toEqual(before);

  expect(consoleErrors).toEqual([]);
});

test("Space ticks the checkbox the caret is in", async ({
  page,
  consoleErrors,
}) => {
  // Word's keyboard half of the same gesture, and the only way to reach the
  // control without a pointer.
  await openForm(page);
  const before = await boxes(page);
  await focusCheckbox(page);

  // The caret is in the control; Space must tick rather than type.
  await page.keyboard.press("Space");
  await expect
    .poll(async () => (await boxes(page)).checked)
    .toBe(before.checked + 1);
  const text = await page.locator("#a11yDocument").innerText();
  expect(text, "Space must not have typed a space into the form").not.toContain(
    "Tick one  ",
  );

  expect(consoleErrors).toEqual([]);
});

test("a form checkbox is read-only in Viewing mode", async ({
  page,
  consoleErrors,
}) => {
  // It is a document mutation and is gated like every other one — the failure
  // to avoid is a control that looks live in a read-only document.
  await openForm(page);
  const before = await boxes(page);

  // The point is resolved AFTER the switch: changing review mode shows or
  // hides the review gutter, which moves the page sideways, so a point taken
  // before the switch lands somewhere else entirely.
  await setReviewMode(page, "viewing");
  await expect(page.locator("#viewingBanner")).toBeVisible();
  const at = await focusCheckbox(page);
  await page.mouse.click(at.x, at.y);

  await expect(page.locator("#status")).toContainText("read-only");
  expect(await boxes(page)).toEqual(before);

  expect(consoleErrors).toEqual([]);
});
