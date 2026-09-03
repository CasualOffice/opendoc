// Guards for a batch of fixes that shipped without any (docs/104 HF-059, HF-065,
// HF-074, HF-076). They share one shape: a capability existed, and the user
// could not reach it or could not tell it had failed.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

async function openContextMenu(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const box = await page.locator("#pages").boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + 120, { button: "right" });
  const menu = page.locator(".editor-context-menu");
  await expect(menu).toBeVisible();
  return menu;
}

// HF-076. This repo's recurring defect is a capability wired to exactly one
// surface, and the right-click menu was the clearest case: seven commands
// declared `contextMenu: true` and the builder hand-picked three of them, so
// "Paste without formatting" and "Select all" — both top-five actions in Word
// and Docs — were declared for the menu and absent from it.
//
// The fix reads membership from the declaration instead of a hand-written list,
// so this asserts the whole declared set rather than the two that were missing.
// A hand-picked builder can satisfy any single label; only the full set catches
// the next one someone forgets to copy across.
test("every command declared for the context menu is in the context menu", async ({
  page,
  consoleErrors,
}) => {
  const menu = await openContextMenu(page);

  // Anchored on `data-command-id`, not on label text: labels are context
  // sensitive ("Undo" becomes "Undo typing") and carry their shortcut in the
  // same box, so matching them by name is both brittle and ambiguous — a naive
  // /^Paste/ matches "Paste without formatting" too.
  const offered = await menu.locator("[data-command-id]").evaluateAll((els) =>
    els.map((el) => el.dataset.commandId),
  );

  for (const id of [
    "edit.undo",
    "edit.redo",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "edit.pasteText",
    "edit.selectAll",
  ]) {
    expect(
      offered,
      `${id} declares contextMenu: true but is not offered in the menu`,
    ).toContain(id);
  }

  expect(consoleErrors).toEqual([]);
});

// HF-076, the same defect in the other direction: the engine has had checklists
// for as long as the ribbon button has, but the list submenu offered only
// Bulleted and Numbered, so a user browsing the menus concluded the editor had
// none.
test("the list submenu offers checklists, not just bullets and numbers", async ({
  page,
  consoleErrors,
}) => {
  const menu = await openContextMenu(page);
  await menu.getByRole("menuitem", { name: /List & indentation/ }).hover();

  const checklist = page.locator('[data-command-id="paragraph.list.checklist"]');
  await expect(checklist, "the list submenu still hides checklists").toHaveCount(1);
  // Present but permanently greyed would be the same defect wearing a disguise.
  await expect(checklist).toBeEnabled();

  expect(consoleErrors).toEqual([]);
});

// HF-074, WCAG 2.4.1 Level A. Reaching the document by keyboard meant tabbing
// past Search, Save, Properties, Settings, eight menus, five ribbon tabs and the
// whole Home band — roughly 150 controls. The link has to be the FIRST tab stop
// to be worth anything, so that is what is asserted, not merely its existence.
test("the first Tab reaches a skip link that lands in the document", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  // Tab from a clean focus state, which is what a fresh load gives a keyboard
  // user. Clicking first would start sequential navigation from the click
  // target, and every clickable point in the chrome is AFTER the link in DOM
  // order — so Tab would move past it and the test would be measuring the wrong
  // journey entirely.
  await page.evaluate(() => document.activeElement?.blur());
  await page.keyboard.press("Tab");

  const focused = page.locator(":focus");
  await expect(focused).toHaveClass(/skip-link/);
  await expect(focused).toBeVisible(); // off-screen until focused, then on top

  await page.keyboard.press("Enter");
  // The link hands over to the editing surface itself, which is already a tab
  // stop — landing on the chrome again would defeat the whole point.
  await expect(page.locator("#pages")).toBeFocused();

  expect(consoleErrors).toEqual([]);
});

// HF-065. A file the browser cannot read — moved, renamed, or revoked between
// the drop and the read — threw inside an un-awaited promise. The editor did
// nothing and said nothing: no status, no error, no clue.
//
// No `consoleErrors` assertion: reporting the reason on the console is correct
// here, and its absence would be the defect.
test("a file that cannot be read reports it instead of failing silently", async ({
  page,
}) => {
  await gotoEditor(page);

  await page.evaluate(() => {
    const transfer = new DataTransfer();
    const file = new File([new Uint8Array([0x50, 0x4b])], "vanished.docx");
    // Exactly the failure the fix is about: the File handle is valid enough to
    // be dropped, and reading its bytes rejects.
    Object.defineProperty(file, "arrayBuffer", {
      value: () => Promise.reject(new DOMException("not found", "NotFoundError")),
    });
    transfer.items.add(file);
    document
      .getElementById("viewport")
      .dispatchEvent(
        new DragEvent("drop", { dataTransfer: transfer, bubbles: true, cancelable: true }),
      );
  });

  await expect(page.locator("#status")).toContainText(/could not be read|could not be opened/i);
  await expect(page.locator("#status")).toContainText(/vanished\.docx|file/i);
});

// HF-057, and the one row in this batch that could lose data rather than merely
// hide a capability. The inspector filled its fields once, on the opening call,
// and never again. Two consequences, and the second is the dangerous one:
//
//   * a drag-resize left the panel showing the pre-drag numbers, so the next
//     Apply — or a nudge — put the object back where it had been;
//   * selecting a SECOND object left the FIRST one's size, alt text and wrap
//     sitting in the fields, now aimed at the new object. Apply then wrote one
//     object's geometry onto another.
//
// The fields are asserted against the object the panel claims to describe, since
// that pairing is exactly what came apart.
test("the object inspector follows the selection instead of describing the last object", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return status && status.textContent === "" && document.querySelectorAll(".page-wrap").length > 0;
    },
    null,
    { timeout: 45_000 },
  );

  // Select the fixture's floating image and open the inspector on it.
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.click({ position: { x: box.width * 0.14, y: box.height * 0.11 } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await page.locator('.object-bar-btn[aria-label="Open object properties"]').click();

  const panel = page.locator(".object-inspector");
  await expect(panel).toBeVisible();
  await expect(panel.locator("[data-object-inspector-kind]")).toHaveText("Image");
  const imageWidth = await panel.locator("[data-object-prop=width]").inputValue();
  expect(Number(imageWidth)).toBeGreaterThan(0);

  // Now select a different object and give it a size the image cannot share, so
  // the assertion cannot pass by coincidence — the fixture's image and a default
  // ellipse are both 2in wide, which made an earlier version of this test green
  // against the bug it was written to catch.
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertShapeBtn").click();
  await page.locator('#shapeGalleryMenu [data-shape-geometry="ellipse"]').click();
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "shape");
  await expect(panel.locator("[data-object-inspector-kind]")).toHaveText("Shape");

  const SHAPE_WIDTH = "3.5";
  await panel.locator("[data-object-prop=width]").fill(SHAPE_WIDTH);
  await panel.locator("[data-object-inspector-apply]").click();
  await expect(panel.locator("[data-object-prop=width]")).toHaveValue(SHAPE_WIDTH);

  // Back to the image. The panel must describe IT again — before the fix the
  // fields kept the last object's numbers, so Apply here wrote the ellipse's
  // 3.5in geometry onto the image.
  await canvas.click({ position: { x: box.width * 0.14, y: box.height * 0.11 } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "image");
  await expect(panel.locator("[data-object-inspector-kind]")).toHaveText("Image");
  await expect(
    panel.locator("[data-object-prop=width]"),
    "the inspector kept the previously selected object's width",
  ).toHaveValue(imageWidth);

  expect(consoleErrors).toEqual([]);
});
