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

/** How many of each box the document currently holds, read from what
 *  assistive technology is told.
 *
 *  This used to count the PRIVATE-USE code points `F0A3`/`F052` in the
 *  mirror's text, because that is how Word writes a symbol-font glyph in
 *  content. Those code points are no longer there, and their absence is the
 *  point: a screen reader announces nothing for a private-use code point, so
 *  the projection now exposes the control as `role="checkbox"` with an
 *  `aria-checked` state and a name (`docs/120`).
 *
 *  Reading the exposed state is not a retreat to "a flag moved": the flag and
 *  the CONTENT glyph are pinned to each other natively, in
 *  `a_form_checkbox_ticks_and_unticks`, which asserts the run inside the
 *  control becomes the declared symbol. This asserts the other half — that
 *  what a person is told matches it. */
async function boxes(page) {
  const all = page.locator("#a11yDocument [role=checkbox]");
  return {
    unchecked: await all.and(page.locator("[aria-checked=false]")).count(),
    checked: await all.and(page.locator("[aria-checked=true]")).count(),
  };
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

test("a form checkbox is announced as a checkbox, named by the text beside it", async ({
  page,
  consoleErrors,
}) => {
  // `docs/118` §3 row 2, `docs/120`. Measured on the owner's Medical Incident
  // Report form before this: the mirror carried the raw `U+F0A3` / `U+F052`
  // Wingdings 2 code points, which a screen reader reads as NOTHING, and no
  // `role` attribute appeared anywhere in `#a11yDocument`. Since #578 the
  // control can be ticked, so an operable unnamed control was the worse
  // failure — WCAG 4.1.2 Name, Role, Value.
  await openForm(page);
  const mirror = page.locator("#a11yDocument");
  const controls = mirror.locator("[role=checkbox]");
  await expect(controls).toHaveCount(7);

  // Not one private-use code point survives into what is read aloud.
  const stray = await mirror.evaluate((el) =>
    [...el.innerText].filter((c) => {
      const code = c.codePointAt(0);
      return (
        (code >= 0xe000 && code <= 0xf8ff) ||
        (code >= 0xf0000 && code <= 0xffffd) ||
        (code >= 0x100000 && code <= 0x10fffd)
      );
    }).length,
  );
  expect(stray, "a private-use code point is silence dressed up as text").toBe(0);

  const named = (name) =>
    mirror.locator(`[role=checkbox][aria-label="${name}"]`);
  // The owner's shape: the box alone in a narrow cell, the label in the NEXT
  // cell of the same row. Its `w:alias` is `chk_fall` and must lose — WCAG 2.2
  // SC 2.5.3 wants the name a sighted user can see.
  await expect(named("Fall or Injury")).toHaveCount(1);
  await expect(named("chk_fall")).toHaveCount(0);
  // The label in the paragraph the control sits in.
  await expect(named("Medication error")).toHaveCount(1);
  // Nothing visible in the row: the author's `w:alias` is all there is.
  await expect(named("Consent given")).toHaveCount(1);
  // A label to the LEFT, which is how a right-aligned form is written.
  await expect(named("Witnessed by a colleague")).toHaveCount(1);
  // Nothing anywhere, and two controls sharing one label: named generically
  // rather than wrongly. A wrong name is worse than a generic one.
  await expect(named("Check box")).toHaveCount(3);
  await expect(named("Either one")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("the announced state follows the document when the box is ticked", async ({
  page,
  consoleErrors,
}) => {
  // The state must be read from the model on every rebuild, not written once
  // at open. A projection that announced a stale state would be worse than
  // none: the reader would be told the form says something it does not.
  await openForm(page);
  const first = page
    .locator("#a11yDocument [role=checkbox][aria-label='Medication error']")
    .first();
  await expect(first).toHaveAttribute("aria-checked", "false");

  await focusCheckbox(page);
  await page.keyboard.press("Space");
  await expect(first).toHaveAttribute("aria-checked", "true");

  await page.locator("#undoBtn").click();
  await expect(first).toHaveAttribute("aria-checked", "false");

  expect(consoleErrors).toEqual([]);
});
