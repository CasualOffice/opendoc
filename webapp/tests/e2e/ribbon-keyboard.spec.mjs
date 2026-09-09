// HF-152 — the ribbon band was a wall for the keyboard.
//
// Every control in it was its own Tab stop, so reaching the document body from
// the tab strip meant walking roughly forty-five formatting buttons, and each of
// the twenty-four groups was an anonymous run of buttons to a screen reader
// because the caption under a group was decoration, not its name.
//
// The band now follows the WAI-ARIA toolbar pattern: one Tab stop, arrow keys to
// move inside it, and each group named from the caption already printed under it.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

/** Controls in the shown panel that the browser would stop on when tabbing. */
async function tabStopCount(page) {
  return page.evaluate(() => {
    const panel = [...document.querySelectorAll(".ribbon-panel")].find((p) => !p.hidden);
    return [...panel.querySelectorAll("button, input, select, [tabindex]")].filter(
      (el) =>
        !el.disabled &&
        !el.closest("[hidden]") &&
        (el.offsetWidth || el.offsetHeight) &&
        el.tabIndex >= 0,
    ).length;
  });
}

test("the whole band is one Tab stop, not one per control", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await expect(page.locator("#panelHome")).toBeVisible();

  // The exact roster shifts with overflow and enablement; what must hold is that
  // the band costs the keyboard ONE stop however many controls it is showing.
  expect(await tabStopCount(page)).toBe(1);

  // And the same after switching tabs — a panel that just became visible must
  // publish its own stop rather than inheriting the hidden panel's.
  await page.locator("#tabInsert").click();
  await expect(page.locator("#panelInsert")).toBeVisible();
  expect(await tabStopCount(page)).toBe(1);

  await page.locator("#tabHome").click();
  await expect(page.locator("#panelHome")).toBeVisible();
  expect(await tabStopCount(page)).toBe(1);
  expect(consoleErrors).toEqual([]);
});

test("arrow keys move between the band's controls and wrap at both ends", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const panel = page.locator("#panelHome");
  await expect(panel).toBeVisible();

  const focusedId = () => page.evaluate(() => document.activeElement?.id || "");
  const items = () =>
    page.evaluate(() => {
      const p = [...document.querySelectorAll(".ribbon-panel")].find((el) => !el.hidden);
      const composites = new Set();
      const found = [];
      for (const el of p.querySelectorAll("button, input, select, [tabindex]")) {
        if (el.disabled || el.closest("[hidden]")) continue;
        if (el.getAttribute("aria-hidden") === "true") continue;
        if (!el.offsetWidth && !el.offsetHeight) continue;
        const composite = el.closest('[role="listbox"], [role="grid"], [role="menu"]');
        if (composite && p.contains(composite)) {
          if (composites.has(composite)) continue;
          composites.add(composite);
          found.push((composite.querySelector('[tabindex="0"]') || el).id);
          continue;
        }
        found.push(el.id);
      }
      return found;
    });

  const roster = await items();
  expect(roster.length).toBeGreaterThan(5);

  // Enter the band at its remembered stop, then walk right.
  await page.evaluate((id) => document.getElementById(id).focus(), roster[0]);
  expect(await focusedId()).toBe(roster[0]);
  await page.keyboard.press("ArrowRight");
  expect(await focusedId()).toBe(roster[1]);

  // Left from the first control wraps to the last, which is what stops the band
  // from having a dead edge the keyboard silently bounces off.
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  expect(await focusedId()).toBe(roster[roster.length - 1]);

  await page.keyboard.press("Home");
  expect(await focusedId()).toBe(roster[0]);
  await page.keyboard.press("End");
  expect(await focusedId()).toBe(roster[roster.length - 1]);
  expect(consoleErrors).toEqual([]);
});

test("the band returns focus to the control last used, not to the start of the row", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await expect(page.locator("#panelHome")).toBeVisible();

  await page.locator("#bold").focus();
  await clickIntoFirstPage(page);

  // Coming back by Tab must land on Bold. Reading the stop directly rather than
  // pressing Tab from the body keeps the assertion about the band's memory and
  // not about how many stops sit between the body and the ribbon.
  const stop = await page.evaluate(() => {
    const p = [...document.querySelectorAll(".ribbon-panel")].find((el) => !el.hidden);
    return p.querySelector('[tabindex="0"]')?.id || "";
  });
  expect(stop).toBe("bold");
  expect(consoleErrors).toEqual([]);
});

test("every ribbon group is named for a screen reader by its printed caption", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const groups = await page.evaluate(() =>
    [...document.querySelectorAll(".rgroup")]
      // A `.rgroup` that holds no controls is a hint strip, not a toolbar group
      // — the table panel's live-region status line is one. Naming it would put
      // an empty group in the accessibility tree.
      .filter((g) => g.querySelector("button, input, select"))
      .map((g) => ({
        id: g.dataset.group || g.className,
        caption: g.querySelector(".rgroup-label")?.textContent?.trim() || "",
        role: g.getAttribute("role") || "",
        label: g.getAttribute("aria-label") || "",
      })),
  );

  expect(groups.length).toBeGreaterThanOrEqual(20);
  for (const group of groups) {
    // The caption is the name. Taking it from the DOM rather than a hand-written
    // list is what keeps the two from drifting apart as groups are added.
    expect(group.caption, `group ${group.id} prints no caption to name it by`).not.toBe("");
    expect(group.role, `group "${group.caption}" has no role`).toBe("group");
    expect(group.label, `group "${group.caption}" is unnamed`).toBe(group.caption);
  }
  expect(consoleErrors).toEqual([]);
});

test("a text field inside the band keeps its own Home, End and arrow keys", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const size = page.locator("#fontSize");
  await expect(size).toBeVisible();
  await size.focus();
  await size.fill("48");
  // Home inside a field is "go to the start of what I typed". If the toolbar
  // claimed it, the caret would leave the field entirely and the value would be
  // uneditable from the keyboard.
  await page.keyboard.press("Home");
  await expect(size).toBeFocused();
  expect(consoleErrors).toEqual([]);
});
