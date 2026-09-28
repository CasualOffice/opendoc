// What a `preview` container is, and what a `readonly` one is — asked as what a
// USER can do, not as what is in the DOM.
//
// The owner looked at a preview and found it was not one: "preview is not how it
// works.. it should be just pages.. and nothing, no rulers and nothing.. no
// selection, no panels and nothing. I saw in read only and other as well I was
// able to see table and menus and selection image and also properties and others."
//
// `preview` is granted none of the nineteen regions, so nothing a host could
// withhold explained what was still on screen: the ruler, the caret, the
// right-click menu and object selection are drawn INTO the work area and belonged
// to no region at all. They are four regions now — `ruler`, `caret`, `context`,
// `objects` — and `main.js` consults the composed set at the three gestures that
// enter the work area, because a `display: none` outline over an object that is
// still SELECTED is the "present but unreachable" failure `docs/99` §9.4 records.
//
// THE TWO ROLES ARE NOT THE SAME, and this file would be wrong if it flattened
// them. `readonly` is a published document to READ: it keeps the caret and the
// right-click menu, because selecting text to copy it and right-clicking for Copy
// and Search are reading affordances. What it loses is the EDITING affordances —
// object handles, object properties, a table's structure commands.
import { test, expect, MOD } from "./fixtures.mjs";

// The float fixture puts one floating image near the top-left of page 1, which is
// the thing the owner clicked.
const IMAGE_AT = { fx: 0.14, fy: 0.11 };

async function open(page, mode) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto(`/editor.html?fixture=float&mode=${mode}`);
  await page.waitForFunction(
    () => {
      const s = document.getElementById("status");
      return s && s.textContent === "" && document.querySelectorAll(".page-wrap").length > 0;
    },
    null,
    { timeout: 45_000 },
  );
}

/** Clicks the floating image, and returns the point it clicked so a caller can
 *  right-click the same place. */
async function clickTheImage(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  const at = { x: box.width * IMAGE_AT.fx, y: box.height * IMAGE_AT.fy };
  await canvas.click({ position: at });
  return { canvas, at };
}

/** What the user can actually see and do, read in one pass. Deliberately NOT
 *  "does the element exist": a hidden control that still answers a click is the
 *  failure this file is about. */
const observed = (page) =>
  page.evaluate(() => ({
    objectMode: document.getElementById("pages")?.dataset.objectMode ?? "",
    outlines: document.querySelectorAll(".overlay .object-outline").length,
    handles: document.querySelectorAll(".object-handle").length,
    objectBar: document.querySelectorAll(".object-bar-actions").length,
    ruler: [...document.querySelectorAll(".ruler")].filter(
      (el) => el.getBoundingClientRect().height > 0,
    ).length,
    selected: String(window.getSelection?.() ?? ""),
  }));

test("a preview container is the pages and nothing else", async ({ page, consoleErrors }) => {
  await open(page, "preview");

  // Nothing around the document: the top bar collapses rather than paying 58px
  // for a strip with nothing in it, and neither navigation axis is offered,
  // because a preview has no commands to reach.
  await expect(page.locator("header.bar")).toBeHidden();
  await expect(page.locator(".ribbon")).toBeHidden();
  await expect(page.locator("#appMenuBar")).toBeHidden();
  await expect(page.locator(".rail")).toBeHidden();
  await expect(page.locator(".footer")).toBeHidden();
  await expect(page.locator("#documentState")).toBeHidden();
  await expect(page.locator("#propertiesBtn")).toBeHidden();
  // Nothing OVER the document either: no mode banner (a mode a container cannot
  // change is not a mode) and no margin comment button — two body-level surfaces
  // that outlived every region rule. The status TOAST is the third, and it is not
  // asserted here on purpose: it auto-hides, so by this point it would be hidden
  // whatever the rule said. It is held by `chrome_regions.test.mjs` instead, where
  // deleting its rule really does go red.
  // The pages themselves ARE there — this is a picture of a document, not an
  // empty frame. Without this the rest of the test would pass on a blank page.
  await expect(page.locator(".page-wrap .page").first()).toBeVisible();

  // Clicking the image selects nothing: no mode, no outline, no handles, no bar.
  await clickTheImage(page);
  expect(await observed(page)).toMatchObject({
    objectMode: "",
    outlines: 0,
    handles: 0,
    objectBar: 0,
    ruler: 0,
  });

  // Right-clicking offers no editing menu — and does not SWALLOW the gesture
  // either, so the visitor keeps the browser's own menu. `defaultPrevented` is
  // what says the second half: a menu that merely renders off-screen, or renders
  // and is hidden by a rule, would satisfy "not visible" while the app had still
  // taken the gesture away.
  const { canvas, at } = await clickTheImage(page);
  await page.evaluate(() => {
    window.__ctx = null;
    document.addEventListener(
      "contextmenu",
      (e) => {
        window.__ctx = e.defaultPrevented;
      },
      { once: true },
    );
  });
  await canvas.click({ button: "right", position: at });
  await expect(page.locator(".editor-context-menu")).toBeHidden();
  expect(
    await page.evaluate(() => window.__ctx),
    "a preview must leave the browser's own context menu alone",
  ).toBe(false);

  // No caret, and nothing to copy out of the picture — asked as what a copy
  // GESTURE yields, not as whether an element is in the tree.
  await page.locator(".page-wrap .page").first().click({ position: { x: 120, y: 200 } });
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Shift+ArrowRight");
  await expect(page.locator(".overlay .caret")).toHaveCount(0);
  const copied = await page.evaluate(() => {
    const data = new DataTransfer();
    document.dispatchEvent(
      new ClipboardEvent("copy", { clipboardData: data, bubbles: true, cancelable: true }),
    );
    return data.getData("text/plain");
  });
  expect(copied, "a preview has no selection to copy").toBe("");

  // And no panel can be opened: the outline command is not in the registry at
  // all, because the palette and its chord belong to no region and CSS cannot
  // reach them.
  await page.keyboard.press(`${MOD}+Shift+P`);
  const palette = page.locator("#cmdPalette");
  if (await palette.isVisible()) {
    await page.locator("#cmdInput").fill("outline");
    await expect(page.locator('#cmdList .cmd-item[data-command-id="view.outline"]')).toHaveCount(0);
    await page.keyboard.press("Escape");
  }
  await expect(page.locator("#outlinePanel")).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

test("a readonly container reads the document but cannot reach an object", async ({
  page,
  consoleErrors,
}) => {
  await open(page, "readonly");

  // READING survives, which is the half that must not be flattened into preview:
  // the menu bar (File ▸ Print is `readonly`'s only grant), the rail, the status
  // bar and find.
  await expect(page.locator("#appMenuBar")).toBeVisible();
  await expect(page.locator(".rail")).toBeVisible();
  await expect(page.locator(".footer")).toBeVisible();
  // The ruler is an editing instrument — indents and tab stops — and goes.
  expect((await observed(page)).ruler).toBe(0);

  // A click still places a caret, because a reader selects text to copy it.
  await page.locator(".page-wrap .page").first().click({ position: { x: 120, y: 200 } });
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Shift+ArrowRight");
  const copied = await page.evaluate(() => {
    const data = new DataTransfer();
    document.dispatchEvent(
      new ClipboardEvent("copy", { clipboardData: data, bubbles: true, cancelable: true }),
    );
    return data.getData("text/plain");
  });
  expect(copied.length, "a reader must still be able to copy").toBeGreaterThan(0);

  // But the image cannot be picked up: no mode, no handles, no object bar, and
  // so no properties dialog to reach.
  await clickTheImage(page);
  expect(await observed(page)).toMatchObject({ objectMode: "", handles: 0, objectBar: 0 });

  // And the right-click menu offers READING commands only — Copy and Find — with
  // no structure commands for the table the caret is in and no object rows.
  const { canvas, at } = await clickTheImage(page);
  await canvas.click({ button: "right", position: at });
  const menu = page.locator(".editor-context-menu");
  await expect(menu).toBeVisible();
  const rows = await menu.locator(".menu-item").evaluateAll((els) =>
    els.map((el) => el.dataset.commandId ?? ""),
  );
  expect(rows).toContain("edit.copy");
  expect(rows.filter((id) => id.startsWith("table.") || id.startsWith("object."))).toEqual([]);
  expect(rows).not.toContain("paragraph.properties");
  expect(rows).not.toContain("format.menu");

  expect(consoleErrors).toEqual([]);
});
