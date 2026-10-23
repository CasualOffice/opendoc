// docs/85 Phase A / Slice 2 (P1G-OBJ-SELECT + the §4 grammar): clicking a
// drawing selects it as an OBJECT (distinct from a text caret), the engine draws
// its outline + engine-declared resize handles, and the interaction grammar's
// Escape two-step returns to a text caret.
import { MOD, expect, gotoEditor, stableBox, test } from "./fixtures.mjs";

// The rich producer fixture places an inline image near the top of page 1;
// this fraction of the first page's box lands on it (found empirically, stable
// across runs — the image is a fixed layout object).
const IMAGE_POS = { fx: 0.32, fy: 0.1 };

async function clickImage(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await canvas.click({ position: { x: box.width * IMAGE_POS.fx, y: box.height * IMAGE_POS.fy } });
}

async function clickBodyText(page) {
  // A point well below the image, in ordinary body prose.
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await canvas.click({ position: { x: box.width * 0.2, y: box.height * 0.55 } });
}

test("an inline image exposes only handles its flow anchor can honor", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickImage(page);

  const pages = page.locator("#pages");
  // The object is selected as a unit (not a text caret).
  await expect(pages).toHaveAttribute("data-object-kind", "image");
  await expect(pages).toHaveAttribute("data-object-mode", "selected");
  const node = await pages.getAttribute("data-object-selected");
  expect(node).toBeTruthy();

  // An inline object offers all EIGHT grips, the same set Word, Google Docs and
  // ONLYOFFICE offer (`AscFormat.hitToHandles` has no inline/floating branch).
  // It used to offer three — E, SE, S — on the reasoning that the flow anchor
  // cannot move; the anchor still cannot move, but that constrains what the
  // COMMIT writes, not which grips exist.
  await expect(page.locator(".overlay .object-outline")).toHaveCount(1);
  const handles = page.locator(".overlay .object-handle");
  expect(await handles.evaluateAll((nodes) => nodes.map((node) => node.dataset.handle))).toEqual([
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    // 8 is the ROTATION grip — ONLYOFFICE's own numbering, where `hitToHandles`
    // returns 0..7 for the resize markers and 8 for the rotation one. An inline
    // picture models `a:xfrm` and the layout paints it, so it is offered here.
    "8",
  ]);
  await expect(page.locator(".object-context-bar")).toBeVisible();
  // The bar is a named toolbar about THIS object (`docs/172`). The drag hint it
  // used to carry is the status line's job (asserted below), as in Docs, whose
  // image toolbar carries none.
  await expect(page.locator(".object-context-bar")).toHaveAttribute("aria-label", "Image actions");
  await expect(page.locator(".object-context-bar")).not.toContainText(/coming soon|later editing slice/i);

  expect(consoleErrors).toEqual([]);
});

test("Escape collapses a selected object to a text caret (two-step exit)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickImage(page);
  const pages = page.locator("#pages");
  await expect(pages).toHaveAttribute("data-object-mode", "selected");

  // A leaf image has no edit mode: Escape from "selected" goes straight to a
  // text caret (the handles disappear, no object is selected).
  await page.keyboard.press("Escape");
  await expect(pages).not.toHaveAttribute("data-object-selected", /.*/);
  await expect(page.locator(".overlay .object-handle")).toHaveCount(0);
  await expect(page.locator(".object-context-bar")).toBeHidden();
  // A text caret is now present.
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  expect(consoleErrors).toEqual([]);
});

test("clicking body text deselects the object", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickImage(page);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  await clickBodyText(page);
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-selected", /.*/);
  await expect(page.locator(".overlay .object-handle")).toHaveCount(0);
  // The click placed a text caret instead.
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  expect(consoleErrors).toEqual([]);
});

/** Answers the find bar's status for `query`, and closes the bar again. */
async function findStatusFor(page, query) {
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(query);
  const status = page.locator("#findStatus");
  await expect(status).not.toHaveText("");
  const text = await status.textContent();
  await page.keyboard.press("Escape");
  return text;
}

// The decided behaviour (`docs/109` HF-315, HF-370): a text key on a selected
// INLINE object drops the object selection and types at the caret after it, as
// a picture just inserted with the caret behind it must keep typing working. It
// never replaces the picture (Word's choice), and never drops the key silently.
test("a text key on a selected inline object types through and keeps the object", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickImage(page);
  const pages = page.locator("#pages");
  await expect(pages).toHaveAttribute("data-object-mode", "selected");
  await expect(pages).toHaveAttribute("data-object-kind", "image");

  await page.keyboard.type("XYZ");
  await expect(pages).not.toHaveAttribute("data-object-selected", /.*/);
  expect(await findStatusFor(page, "XYZ"), "the typed text is in the document").not.toBe(
    "No match",
  );

  // The picture is still there: typing went beside it, not over it. Typed in
  // front of it, the text pushes it right along its line by the text's width,
  // so look for it along that line rather than at the old point.
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  let found = false;
  for (let dx = 0; dx <= 120 && !found; dx += 4) {
    await canvas.click({ position: { x: box.width * IMAGE_POS.fx + dx, y: box.height * IMAGE_POS.fy } });
    found = (await pages.getAttribute("data-object-kind")) === "image";
  }
  expect(found, "the inline picture survived the typing").toBe(true);
  await expect(pages).toHaveAttribute("data-object-mode", "selected");

  expect(consoleErrors).toEqual([]);
});

test("a selected floating object swallows text keys, and Delete removes it (undoable)", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  const clickFloat = () => page.mouse.click(box.x + box.width * 0.14, box.y + box.height * 0.11);
  await clickFloat();
  const pages = page.locator("#pages");
  await expect(pages).toHaveAttribute("data-object-mode", "selected");
  expect((await pages.getAttribute("data-object-capabilities")) ?? "").toContain("canMove");

  // A floating object has no place in the text to type beside, so a text key
  // does not edit through it (a stale caret is never mutated): the object stays
  // selected and the typed text is not findable.
  await page.keyboard.type("XYZ");
  await expect(pages).toHaveAttribute("data-object-mode", "selected");
  expect(await findStatusFor(page, "XYZ")).toBe("No match");

  // Delete removes the still-selected object as one undoable action: the
  // object selection clears and there is something to undo.
  await expect(pages).toHaveAttribute("data-object-mode", "selected");
  await page.keyboard.press("Delete");
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-mode", /.*/);
  await expect(page.locator("#undoBtn")).toBeEnabled();

  // One Undo restores it; the object is selectable again.
  await page.locator("#undoBtn").click();
  await clickFloat();
  await expect(pages).toHaveAttribute("data-object-mode", "selected");

  expect(consoleErrors).toEqual([]);
});

test("image-options feedback is transient and clears when object selection exits", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickImage(page);
  await page.keyboard.press("Enter");
  await expect(page.locator("#status")).toContainText("drag its handles to resize");

  await page.keyboard.press("Escape");
  await expect(page.locator("#status")).toHaveText("");
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-selected", /.*/);
  expect(consoleErrors).toEqual([]);
});
