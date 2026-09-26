// `109` UX-021 — the runtime half of the radio-group contract.
//
// The defect: five of seven containers declared `role="radiogroup"` while their
// options were `aria-pressed` toggle buttons, or — before the surface had ever
// been reflected once — buttons with no role and no state at all. A screen reader
// announced "radio group, three items" and then three plain buttons or three
// toggle buttons, and the arrow keys did nothing, so the control could not be
// operated from the keyboard.
//
// What is asserted here is the GUARANTEE, not the helper: whatever code builds a
// segmented control, if it tells assistive technology the thing is a radio group
// then the options must be radios, must carry a checked state, must offer exactly
// one Tab stop, and the arrow keys must actually change the setting. The static
// half (no `aria-pressed` inside a declared radio group in the markup) is
// `tests/radio_group.test.mjs`, which runs in `npm run test:unit`.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

/** Every declared radio group in the page, with what a screen reader would find
 *  in it. Read for ALL of them, hidden ones included: the accessibility tree is
 *  wrong from load, not from the moment a dialog opens, and four of the five
 *  broken groups were broken precisely in the state before their first open. */
function radioGroupReport(page) {
  return page.evaluate(() =>
    [...document.querySelectorAll('[role="radiogroup"]')].map((group) => {
      const options = [...group.querySelectorAll("button")];
      return {
        id: group.id || group.className || group.tagName,
        count: options.length,
        nonRadio: options.filter((o) => o.getAttribute("role") !== "radio").length,
        stateless: options.filter(
          (o) => !["true", "false"].includes(o.getAttribute("aria-checked")),
        ).length,
        alsoToggles: options.filter((o) => o.hasAttribute("aria-pressed")).length,
        tabStops: options.filter((o) => o.tabIndex === 0).length,
      };
    }),
  );
}

test("every declared radio group is one, from load, with no dialog opened", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const groups = await radioGroupReport(page);

  // If the editor stopped declaring radio groups, every assertion below would
  // pass over an empty list and this file would be a decoration.
  expect(groups.length).toBeGreaterThanOrEqual(5);

  for (const group of groups) {
    expect(group.count, `${group.id} has no options`).toBeGreaterThan(0);
    expect(
      group.nonRadio,
      `${group.id}: ${group.nonRadio} of ${group.count} options are not radios, so the ` +
        "group's own role contradicts its children",
    ).toBe(0);
    expect(
      group.stateless,
      `${group.id}: ${group.stateless} options have no aria-checked, so a screen reader ` +
        "cannot say which one is chosen",
    ).toBe(0);
    expect(
      group.alsoToggles,
      `${group.id}: ${group.alsoToggles} options also claim aria-pressed — a radio is not ` +
        "a toggle button, and role=radio does not allow the attribute",
    ).toBe(0);
    expect(
      group.tabStops,
      `${group.id}: ${group.tabStops} Tab stops. A radio group is entered once and ` +
        "navigated with the arrows, so exactly one option holds the stop",
    ).toBe(1);
  }
  expect(consoleErrors).toEqual([]);
});

test("an arrow key operates a radio group, not just its focus ring", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Page setup ▸ Orientation. Chosen because its effect is visible in two other
  // fields: picking landscape swaps the width and height, mirroring Word. So this
  // asserts the setting CHANGED, which no amount of correct ARIA would fake.
  await page.locator("#tabView").click();
  await page.locator("#pageSetupBtn").click();
  await expect(page.locator("#pageSetupMenu")).toBeVisible();

  const chosen = page.locator('#pageOrientationSeg button[aria-checked="true"]');
  await expect(chosen).toHaveCount(1);
  const before = {
    orientation: await chosen.getAttribute("data-orientation"),
    width: await page.locator("#pageWidth").inputValue(),
    height: await page.locator("#pageHeight").inputValue(),
  };

  // The dialog opens with the chosen orientation focused, which is the group's
  // single Tab stop — so the arrows are being pressed exactly where a keyboard
  // user's hands already are.
  await expect(chosen).toBeFocused();
  await page.keyboard.press("ArrowRight");

  const after = {
    orientation: await page
      .locator('#pageOrientationSeg button[aria-checked="true"]')
      .getAttribute("data-orientation"),
    width: await page.locator("#pageWidth").inputValue(),
    height: await page.locator("#pageHeight").inputValue(),
  };
  expect(after.orientation).not.toBe(before.orientation);
  expect(after.width).toBe(before.height);
  expect(after.height).toBe(before.width);
  // Arrowing selects in a radio group; focus follows the selection rather than
  // being left behind on the option the user moved away from.
  await expect(page.locator('#pageOrientationSeg button[aria-checked="true"]')).toBeFocused();

  // And back, so the arrows are a navigation axis rather than a one-way door.
  await page.keyboard.press("ArrowLeft");
  expect(
    await page
      .locator('#pageOrientationSeg button[aria-checked="true"]')
      .getAttribute("data-orientation"),
  ).toBe(before.orientation);
  expect(await page.locator("#pageWidth").inputValue()).toBe(before.width);

  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("a tri-state formatting group stays a group of toggle buttons", async ({ page }) => {
  await gotoEditor(page);
  // Paragraph alignment over a multi-paragraph selection is "mixed", and
  // `aria-checked="mixed"` is not legal on a radio — which is why Word, Google
  // Docs and ONLYOFFICE all use toggle buttons for this exact control
  // (ONLYOFFICE `documenteditor/main/app/view/Toolbar.js:559-607`,
  // `toggleGroup: 'alignGroup'`, with no radiogroup wrapper). Converting it to a
  // radio group would be the same defect with the roles swapped, so the guard
  // runs in both directions.
  const align = page.locator("#paraPanelAlign");
  await expect(align).toHaveAttribute("role", "group");
  const options = align.locator("button");
  const count = await options.count();
  expect(count).toBeGreaterThan(0);
  for (let i = 0; i < count; i += 1) {
    const option = options.nth(i);
    expect(await option.getAttribute("role")).toBeNull();
    expect(["true", "false", "mixed"]).toContain(await option.getAttribute("aria-pressed"));
  }
});
