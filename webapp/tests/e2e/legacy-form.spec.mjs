// Filling a LEGACY form, and not being able to edit anything else.
//
// `docs/109` HF-175. The owner's loan agreement uses the other of Word's two
// form mechanisms: `w:fldChar` + `w:ffData` (FORMCHECKBOX / FORMTEXT), not the
// `w14:checkbox` content control that `form-checkbox.spec.mjs` covers. It also
// declares `w:documentProtection w:edit="forms" w:enforcement="1"`.
//
// Measured on the real file before this change: the 19 legacy checkboxes were
// inert — the gesture only recognised content controls — and the protection
// was read into the model and then ignored, so the whole contract could be
// typed over. A form you cannot fill and a contract you can rewrite: the two
// halves of the same document being wrong in opposite directions.
//
// Word and ONLYOFFICE (`word/Editor/Paragraph/...` field handling, and
// `CDocument.private_CanEditInFormMode`) agree on both behaviours, so this
// asserts what they agree on.
import { test, expect, gotoEditor, stableBox } from "./fixtures.mjs";

const PROTECTED = "../fixtures/generated/forms-protected.docx";

/** The legacy box glyph is synthesised by layout from the field's state, so
 *  the ticked/unticked counts are read from the accessibility mirror — the
 *  same place a screen reader reads, and a place an internal flag cannot
 *  reach on its own. */
async function boxes(page) {
  return page.locator("#a11yDocument").evaluate((el) => {
    const text = el.innerText;
    return {
      unchecked: [...text].filter((c) => c === "□").length,
      checked: [...text].filter((c) => c === "☑").length,
    };
  });
}

async function openForm(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(PROTECTED);
  await expect(page.locator("#a11yDocument")).toContainText("End of form");
}

/** Puts the caret at the start of the given line, by keyboard so no pixel is
 *  guessed, and returns the on-screen point of the glyph that begins there —
 *  read from the caret the editor itself draws. */
async function caretAtLine(page, downs) {
  const mod = process.platform === "darwin" ? "Meta" : "Control";
  await page.locator("#pages").click({ position: { x: 300, y: 40 } });
  await page.keyboard.press(`${mod}+Home`);
  for (let i = 0; i < downs; i += 1) await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Home");
  const caret = page.locator(".overlay .caret").first();
  await expect(caret).toBeVisible();
  const box = await stableBox(caret);
  return { x: box.x + 3, y: box.y + box.height / 2 };
}

test("a legacy FORMCHECKBOX ticks on click, even in a protected document", async ({
  page,
  consoleErrors,
}) => {
  await openForm(page);
  const before = await boxes(page);
  expect(
    before.unchecked,
    "the fixture must start with an empty legacy box",
  ).toBeGreaterThan(0);

  const at = await caretAtLine(page, 1);
  await page.mouse.click(at.x, at.y);

  await expect.poll(async () => (await boxes(page)).checked).toBe(
    before.checked + 1,
  );
  // Protection must not have blocked it: filling a form is what the protection
  // exists to ALLOW.
  await expect(page.locator("#status")).not.toContainText("protected");

  expect(consoleErrors).toEqual([]);
});

test("typing outside a form field in a protected document says why", async ({
  page,
  consoleErrors,
}) => {
  await openForm(page);
  const before = await page.locator("#a11yDocument").innerText();

  // Ordinary body text, well away from any field. Reached by keyboard: the
  // visible text lives on a canvas, and `getByText` finds only the offscreen
  // accessibility mirror, which is not where a person clicks.
  await caretAtLine(page, 3);
  await page.keyboard.press("End");
  await page.keyboard.type("QZX");

  // The document is unchanged...
  await expect(page.locator("#a11yDocument")).not.toContainText("QZX");
  expect(await page.locator("#a11yDocument").innerText()).toBe(before);
  // ...and the reader is told the DOCUMENT is locked, not sent to inspect a
  // selection that is perfectly fine.
  await expect(page.locator("#status")).toContainText("protected");
  await expect(page.locator("#status")).not.toContainText("selection");

  expect(consoleErrors).toEqual([]);
});
