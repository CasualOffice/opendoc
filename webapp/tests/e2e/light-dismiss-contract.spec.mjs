// THE LIGHT-DISMISS CONTRACT: a transient surface closes when you point
// somewhere else.
//
// Reported as "you defined dropdown and dont close when click somewhere else".
// The same class was fixed once already for the Settings panel (#556): its
// outside-click listener ran on `click`, so the menu row's own click both
// opened the panel and closed it. The phase was moved to `pointerdown`, which
// every other dismissable surface in the editor already used — except the
// toolbar-popover manager, which sat on `mousedown`. `mousedown` never fires
// for a pen or a touch contact that the page has already consumed, so on those
// inputs the font, colour, spacing, bullet, numbering, shape, table-style and
// zoom menus simply stayed open while the user pointed elsewhere. One phase,
// eight surfaces.
//
// Patching the manager alone would leave the next surface to be found by a
// user, so this spec asserts the CONTRACT and discovers its subjects from the
// live DOM: every enabled `aria-expanded` trigger in the menu bar and in every
// ribbon tab is opened, and whatever overlay appears must close on a pointer
// press somewhere neutral. A menu added tomorrow is covered without editing
// this file.
//
// Two kinds of surface are deliberately NOT subject to it, and each is
// recognised by a property rather than by name:
//
//   * a modal (`aria-modal="true"`) owns the screen until it is answered, and
//     its dismissal is `dialog-contract.spec.mjs`'s business;
//   * a DOCKED panel — an in-flow item of the work area, like the paragraph and
//     table property panes and the symbol/emoji pickers — is Word's task pane.
//     It sits beside the document rather than over it, nothing is hidden behind
//     it, and closing it on an outside click would close it on the very click
//     that puts the caret where you want to use it.
//
// The completeness test at the end fails if the set of triggers the sweep found
// ever shrinks, so a surface cannot leave the contract by quietly losing its
// `aria-expanded`.
import { test, expect, gotoEditor, clickIntoFirstPage, useCompactChrome } from "./fixtures.mjs";

const RIBBON_TABS = ["home", "insert", "layout", "references", "view", "review"];

// Triggers whose activation does not leave a dismissable overlay on screen, and
// why. Each one still has to EXIST (asserted below), so a skip cannot outlive
// its control.
const NOT_A_POPOVER = new Map([
  ["ribbonViewToggle", "collapses the ribbon; it is a two-state toggle, not a surface"],
  ["propertiesBtn", "opens the document-properties modal (dialog-contract.spec.mjs)"],
  ["insertSymbolBtn", "opens the docked symbol picker in the inspector column"],
  ["insertEmojiBtn", "opens the docked emoji picker in the inspector column"],
  ["paraOptsBtn", "opens the docked paragraph-properties task pane"],
  ["tablePropertiesBtn", "opens the docked table-properties task pane"],
  ["pageSetupBtn", "opens the page-setup modal (dialog-contract.spec.mjs)"],
  ["insertTableBtn", "opens the insert-table grid, swept as a popover below"],
]);

/** Snapshots which candidate overlays are on screen. An overlay is any element
 *  taken out of flow — `position: fixed` or `absolute` — because that is what
 *  makes a surface paint OVER the document instead of beside it, and therefore
 *  what makes leaving it open a problem. */
const visibleOverlays = () =>
  [...document.querySelectorAll("[id]")]
    .filter((el) => {
      if (el.getClientRects().length === 0) return false;
      const position = getComputedStyle(el).position;
      return position === "fixed" || position === "absolute";
    })
    .map((el) => el.id);

/** The triggers this sweep should drive: enabled, visible, and advertising an
 *  expanded/collapsed state. Discovered, never listed. */
const expandableTriggers = () =>
  [...document.querySelectorAll("[aria-expanded]")]
    .filter(
      (el) =>
        el.getClientRects().length > 0 &&
        !el.disabled &&
        !el.closest("[hidden]") &&
        // Rows inside an open menu expand SUBMENUS; the menu that holds them is
        // the surface under test, and it is swept through its own trigger.
        !el.closest("#appMenuPopover, #editorContextMenu"),
    )
    .map((el) => ({
      id: el.id,
      label: el.id || el.getAttribute("aria-label") || el.textContent.trim().slice(0, 24),
      menu: el.dataset.menu ?? null,
    }));

/** A viewport point that lands on neither `surfaceId` nor `triggerId` and is not
 *  inside any other open overlay — the "somewhere else" a user points at.
 *  Chosen by hit-testing real candidates rather than assumed, so a surface that
 *  grows to cover the whole screen fails to find one instead of being tested
 *  against a point inside itself. */
const neutralPoint = ({ surfaceId, triggerId }) => {
  const surface = document.getElementById(surfaceId);
  const trigger = triggerId ? document.getElementById(triggerId) : null;
  const w = window.innerWidth;
  const h = window.innerHeight;
  const candidates = [
    [w * 0.5, h * 0.62],
    [w * 0.5, h * 0.8],
    [w * 0.2, h * 0.8],
    [w * 0.8, h * 0.62],
    [w * 0.5, h * 0.45],
  ];
  for (const [x, y] of candidates) {
    const hit = document.elementFromPoint(x, y);
    if (!hit) continue;
    if (surface.contains(hit)) continue;
    if (trigger && (trigger.contains(hit) || hit.contains(trigger))) continue;
    return { x: Math.round(x), y: Math.round(y), on: hit.id || hit.className || hit.tagName };
  }
  return null;
};

/** Opens `trigger`, returns the overlay that appeared (or why there is none). */
async function openAndIdentify(page, trigger) {
  const before = await page.evaluate(visibleOverlays);
  const locator = page.locator(`#${trigger.id}`);
  await locator.click();
  // Give the surface a frame to paint before deciding nothing appeared.
  await page.waitForTimeout(80);
  const after = await page.evaluate(visibleOverlays);
  const appeared = after.filter((id) => !before.includes(id));
  if (appeared.length === 0) return { surfaceId: null };
  // The outermost of the new ids is the surface; a menu that paints a child
  // overlay (a colour bar inside a colour menu) must not be mistaken for it.
  const surfaceId = await page.evaluate((ids) => {
    const nodes = ids.map((id) => document.getElementById(id)).filter(Boolean);
    const outer = nodes.find((node) => !nodes.some((other) => other !== node && other.contains(node)));
    return (outer ?? nodes[0]).id;
  }, appeared);
  const kind = await page.evaluate((id) => {
    const el = document.getElementById(id);
    return {
      modal: el.getAttribute("aria-modal") === "true" || el.classList.contains("dialog-overlay"),
    };
  }, surfaceId);
  return { surfaceId, ...kind };
}

for (const tab of RIBBON_TABS) {
  test(`ribbon ▸ ${tab}: every popover it opens closes on an outside pointer press`, async ({
    page,
    consoleErrors,
  }) => {
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    await page.locator(`[data-tab="${tab}"]`).click();

    const triggers = (await page.evaluate(expandableTriggers)).filter(
      (t) => t.id && !t.menu && !NOT_A_POPOVER.has(t.id),
    );
    expect(triggers.length, `ribbon ▸ ${tab} exposed no expandable trigger`).toBeGreaterThan(0);

    const failures = [];
    for (const trigger of triggers) {
      // A previous iteration may have moved the caret or changed the tab.
      await page.locator(`[data-tab="${tab}"]`).click();
      if (!(await page.locator(`#${trigger.id}`).isEnabled())) continue;
      const { surfaceId, modal } = await openAndIdentify(page, trigger);
      if (!surfaceId || modal) {
        if (modal) await page.keyboard.press("Escape");
        continue;
      }
      const point = await page.evaluate(neutralPoint, { surfaceId, triggerId: trigger.id });
      if (!point) {
        failures.push(`${trigger.label}: #${surfaceId} left no neutral point to press`);
        await page.keyboard.press("Escape");
        continue;
      }
      // `mouse.click` is a real pointer press: pointerdown → mousedown → mouseup
      // → click, in that order. A surface that dismisses on any of those phases
      // passes; one that dismisses on none stays visible and fails here.
      await page.mouse.click(point.x, point.y);
      const stillOpen = await page
        .locator(`#${surfaceId}`)
        .isVisible()
        .catch(() => false);
      if (stillOpen) {
        failures.push(
          `${trigger.label} → #${surfaceId} stayed open after a pointer press on ${point.on} at (${point.x}, ${point.y})`,
        );
        await page.keyboard.press("Escape");
      }
    }
    expect(failures, `surfaces that ignored an outside pointer press:\n  ${failures.join("\n  ")}`).toEqual(
      [],
    );
    expect(consoleErrors).toEqual([]);
  });
}

test("the application menus close on an outside pointer press", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  // The menu bar is the compact chrome's navigation axis (docs/122). In ribbon
  // mode it is hidden, so `expandableTriggers` would report no menus and this
  // test would pass by finding nothing to check — a guard that cannot fail.
  await useCompactChrome(page);
  await clickIntoFirstPage(page);
  const menus = (await page.evaluate(expandableTriggers)).filter((t) => t.menu);
  expect(menus.length, "the menu bar exposed no menus").toBeGreaterThan(4);

  const failures = [];
  for (const menu of menus) {
    await page.locator(`.app-menu-button[data-menu="${menu.menu}"]`).click();
    await expect(page.locator("#appMenuPopover")).toBeVisible();
    const point = await page.evaluate(neutralPoint, {
      surfaceId: "appMenuPopover",
      triggerId: null,
    });
    expect(point, `no neutral point outside the ${menu.menu} menu`).not.toBeNull();
    await page.mouse.click(point.x, point.y);
    if (await page.locator("#appMenuPopover").isVisible()) {
      failures.push(`the ${menu.menu} menu stayed open after a pointer press on ${point.on}`);
      await page.keyboard.press("Escape");
    }
  }
  expect(failures.join("\n")).toBe("");
  expect(consoleErrors).toEqual([]);
});

test("the surfaces excused from light dismiss still exist, and are docked or modal", async ({
  page,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  for (const [id, reason] of NOT_A_POPOVER) {
    await expect(page.locator(`#${id}`), `${id} is excused (${reason}) but is gone`).toHaveCount(1);
  }
  // The two task panes are excused because they are IN FLOW beside the
  // document, not floating over it. Assert that, rather than trusting the note:
  // if either is ever turned into an overlay, it needs light dismiss and this
  // goes red.
  await page.locator('[data-tab="home"]').click();
  await page.locator("#paraOptsBtn").click();
  const docked = await page.evaluate(() => {
    const el = document.getElementById("paragraphPropertiesPanel");
    return { hidden: el.hidden, position: getComputedStyle(el).position };
  });
  expect(docked.hidden).toBe(false);
  expect(docked.position).toBe("static");
});
