// Every dialog has ONE anatomy (`docs/109` UX-053, `docs/184` D1, approved by
// the owner 2026-11-01).
//
// The audit measured 51 dialogs in 16 widths, 8 header heights (22-116px), 11
// section gaps, 13px titles, buttons 34px tall beside 36px inputs, and seven
// different ways to finish a footer. Each dialog had been tuned on its own, so
// the product read as fifty small designs. This spec opens every dialog in the
// roster (`modal-roster.mjs`, which `dialog-contract.spec.mjs` proves complete
// against `editor.html`) and asserts the anatomy as a browser lays it out:
//
//   * at 1280x720 the card is 400, 560 or 720px wide (S/M/L) and nothing else;
//     at 390x844 it is a full-width bottom sheet;
//   * the title is 20px (1.25rem);
//   * the footer is painted inside the card and inside the window — pinned,
//     not scrolled away;
//   * no button label wraps, and nothing scrolls sideways;
//   * every input, select, dialog button and segmented control is the same
//     height (36px with a fine pointer).
//
// Both themes are measured with the card open: the theme changes no length,
// and this is what proves it.
import { expect, gotoEditor, test } from "./fixtures.mjs";
import { DIALOGS } from "./modal-roster.mjs";

const LAPTOP = { width: 1280, height: 720 };
const PHONE = { width: 390, height: 844 };
const RUNGS = [400, 560, 720];

/** Everything the anatomy promises, measured in the page. */
async function measure(page, id) {
  return page.evaluate((dialogId) => {
    const root = document.getElementById(dialogId);
    const card = root.querySelector(".dialog-card");
    const shown = (el) => el.getClientRects().length > 0 && getComputedStyle(el).visibility !== "hidden";
    const box = card.getBoundingClientRect();
    const title = card.querySelector(".dialog-heading h2, .dialog-head h2");
    const foot = card.querySelector(":scope > .dialog-foot");
    const footBox = foot && shown(foot) ? foot.getBoundingClientRect() : null;
    const buttons = [...card.querySelectorAll("button")].filter(shown);
    const wrapped = buttons
      .filter((b) => b.classList.contains("dialog-button"))
      .filter((b) => {
        const range = document.createRange();
        range.selectNodeContents(b);
        const lines = new Set([...range.getClientRects()].map((r) => Math.round(r.top)));
        return lines.size > 1;
      })
      .map((b) => b.textContent.trim());
    const controls = [
      ...card.querySelectorAll(
        ".dialog-button, .dialog-select, .dialog-field > select, .number-control, .dialog-body .segmented:not(.segmented-stacked), " +
          '.dialog-field > input:not([type="checkbox"]):not([type="radio"]):not([type="color"]):not([type="range"]):not([type="file"])',
      ),
    ]
      .filter(shown)
      .filter((el) => !el.closest(".number-control") || el.classList.contains("number-control"))
      .map((el) => ({
        what: `${el.tagName.toLowerCase()}${el.id ? `#${el.id}` : ""}.${[...el.classList].join(".")}`,
        height: Math.round(el.getBoundingClientRect().height),
      }));
    const overflow = [card, ...card.querySelectorAll(".dialog-body, .dialog-foot, .dialog-head")]
      .filter(shown)
      .filter((el) => el.scrollWidth > el.clientWidth + 1)
      .map((el) => `${el.className} ${el.scrollWidth}>${el.clientWidth}`);
    const input = Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--h-input")) * 16;
    return {
      width: Math.round(box.width),
      left: Math.round(box.left),
      top: Math.round(box.top),
      bottom: Math.round(box.bottom),
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
      titleSize: title ? Number.parseFloat(getComputedStyle(title).fontSize) : null,
      foot: footBox ? { top: Math.round(footBox.top), bottom: Math.round(footBox.bottom) } : null,
      wrapped,
      controls,
      overflow,
      input,
    };
  }, id);
}

function assertAnatomy(m, id, where) {
  expect(m.titleSize, `#${id} (${where}) title must be 20px`).toBe(20);
  expect(m.wrapped, `#${id} (${where}) wraps a button label`).toEqual([]);
  expect(m.overflow, `#${id} (${where}) scrolls sideways`).toEqual([]);
  if (m.foot) {
    expect(m.foot.bottom, `#${id} (${where}) footer is below the card`).toBeLessThanOrEqual(m.bottom + 1);
    expect(m.foot.bottom, `#${id} (${where}) footer is below the window`).toBeLessThanOrEqual(m.viewportHeight);
    expect(m.foot.top, `#${id} (${where}) footer is above the card`).toBeGreaterThanOrEqual(m.top);
  }
  const off = m.controls.filter((c) => Math.abs(c.height - m.input) > 1);
  expect(off, `#${id} (${where}) has a control that is not ${m.input}px tall`).toEqual([]);
}

for (const dialog of DIALOGS()) {
  test(`${dialog.name}: the dialog anatomy holds at 1280x720 in both themes`, async ({ page, consoleErrors }) => {
    await page.setViewportSize(LAPTOP);
    await dialog.open(page);
    await expect(page.locator(`#${dialog.id}`)).toBeVisible();
    for (const theme of ["light", "dark"]) {
      await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
      const m = await measure(page, dialog.id);
      expect(RUNGS, `#${dialog.id} (${theme}) is ${m.width}px wide, not S/M/L`).toContain(m.width);
      assertAnatomy(m, dialog.id, `1280 ${theme}`);
    }
    expect(consoleErrors).toEqual([]);
  });

  test(`${dialog.name}: the dialog is a full-width sheet at 390x844 in both themes`, async ({ page }) => {
    await page.setViewportSize(PHONE);
    await gotoEditor(page);
    // Revealed the way `registerModal.open()` does, as `phone-dialog-sheet`
    // does: several routes are ribbon journeys a phone does not have.
    await page.evaluate((id) => {
      document.getElementById(id).hidden = false;
      document.body.classList.add("modal-open");
    }, dialog.id);
    for (const theme of ["light", "dark"]) {
      await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
      const m = await measure(page, dialog.id);
      expect(m.width, `#${dialog.id} (${theme}) is not full width on a phone`).toBe(m.viewportWidth);
      expect(m.left).toBe(0);
      assertAnatomy(m, dialog.id, `390 ${theme}`);
    }
  });
}
