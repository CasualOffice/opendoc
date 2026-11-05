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
//   * at 1280x720 the card is 400, 560 or 720px wide (S/M/L), or 960px for the one
//     reference card (Keyboard shortcuts), and nothing else;
//     at 390x844 it is a full-width bottom sheet;
//   * the title is 15px (--fs-lg; owner, 2026-11-02: 20px was too large);
//   * the footer is painted inside the card and inside the window — pinned,
//     not scrolled away;
//   * no button label wraps, and nothing scrolls sideways;
//   * every input, select, dialog button and segmented control is the same
//     height (36px with a fine pointer);
//   * the description under the title is at most ONE line (D1: "help text is at
//     most one line"; audit DIALOGS-13 measured ten dialogs wrapping to two);
//   * the footer holds actions and nothing else: no filler sentence such as
//     "One undoable action." (DIALOGS-13). Text is allowed only in a button, a
//     labelled footer field (Page setup's Apply to), a live status
//     (`role="status"`, empty until something happens) or the one declared
//     note, Settings' "Changes apply as you make them" (`data-foot-note`).
//
// Settings keeps ONE frame on every page (D3, DIALOGS-09): the card measures
// the same height whichever page is chosen.
//
// Both themes are measured with the card open: the theme changes no length,
// and this is what proves it.
import { expect, gotoEditor, test } from "./fixtures.mjs";
import { DIALOGS } from "./modal-roster.mjs";

const LAPTOP = { width: 1280, height: 720 };
const PHONE = { width: 390, height: 844 };
const RUNGS = [400, 560, 720, 960];

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
        // Lines are runs of TEXT whose boxes overlap vertically. An icon glyph
        // beside a label sits at a different top without being a second line,
        // so icon fonts (`.ms`) are left out and boxes are merged by overlap.
        const boxes = [];
        const walker = document.createTreeWalker(b, NodeFilter.SHOW_TEXT);
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
          if (!node.textContent.trim() || node.parentElement?.closest(".ms")) continue;
          const range = document.createRange();
          range.selectNodeContents(node);
          boxes.push(...[...range.getClientRects()].filter((r) => r.width > 0));
        }
        boxes.sort((a, c) => a.top - c.top);
        let lines = 0;
        let bottom = -Infinity;
        for (const r of boxes) {
          if (r.top >= bottom - 1) lines += 1;
          bottom = Math.max(bottom, r.bottom);
        }
        return lines > 1;
      })
      .map((b) => b.textContent.trim());
    const controls = [
      ...card.querySelectorAll(
        ".dialog-button, .dialog-select, .dialog-field > select, .number-control, .dialog-body .segmented:not(.segmented-stacked), .settings-input, .color-select, " +
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
    // The description: one line, measured as line boxes, so a long word that
    // merely touches the edge is not counted twice.
    const description = card.querySelector(".dialog-heading p");
    let descriptionLines = 0;
    if (description && shown(description) && description.textContent.trim()) {
      const range = document.createRange();
      range.selectNodeContents(description);
      descriptionLines = new Set([...range.getClientRects()].filter((r) => r.width > 0).map((r) => Math.round(r.top))).size;
    }
    // Filler: visible footer text that is not an action, a labelled field, a
    // live status, or the declared note.
    const filler = [];
    if (foot && shown(foot)) {
      const walker = document.createTreeWalker(foot, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const text = node.textContent.trim();
        const host = node.parentElement;
        if (!text || !host || !shown(host)) continue;
        if (host.closest("button, select, option, .dialog-foot-field, [role='status'], [data-foot-note]")) continue;
        filler.push(text);
      }
    }
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
      descriptionLines,
      filler,
    };
  }, id);
}

function assertAnatomy(m, id, where) {
  expect(m.titleSize, `#${id} (${where}) title must be 15px`).toBe(15);
  expect(m.wrapped, `#${id} (${where}) wraps a button label`).toEqual([]);
  expect(m.overflow, `#${id} (${where}) scrolls sideways`).toEqual([]);
  if (m.foot) {
    expect(m.foot.bottom, `#${id} (${where}) footer is below the card`).toBeLessThanOrEqual(m.bottom + 1);
    expect(m.foot.bottom, `#${id} (${where}) footer is below the window`).toBeLessThanOrEqual(m.viewportHeight);
    expect(m.foot.top, `#${id} (${where}) footer is above the card`).toBeGreaterThanOrEqual(m.top);
  }
  const off = m.controls.filter((c) => Math.abs(c.height - m.input) > 1);
  expect(off, `#${id} (${where}) has a control that is not ${m.input}px tall`).toEqual([]);
  // Measured on the laptop, where the audit measured it; a phone sheet is
  // narrower than any rung and may wrap the same sentence.
  // The confirmation dialog is exempt: its "description" IS the question, set by
  // whoever asks it, not help text under a title.
  if (where.startsWith("1280") && id !== "confirmDialog") {
    expect(m.descriptionLines, `#${id} (${where}) has a description longer than one line`).toBeLessThanOrEqual(1);
  }
  expect(m.filler, `#${id} (${where}) has filler text in its footer`).toEqual([]);
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

// D3 / DIALOGS-09: the audit measured the Settings card at 776, 608, 562 and
// 701px on its four pages, so the footer and the close button jumped on every
// click. One frame now: every page measures the same.
test("Settings: the frame is the same height on every page", async ({ page }) => {
  await page.setViewportSize(LAPTOP);
  await gotoEditor(page);
  await page.locator("#settingsBtn").click();
  await expect(page.locator("#settingsPanel")).toBeVisible();
  const tabs = await page.locator("#settingsPanel [data-settings-tab]").evaluateAll((els) => els.map((el) => el.dataset.settingsTab));
  expect(tabs).toEqual(["general", "proofing", "autocorrect", "saving", "ai", "about"]);
  const heights = {};
  for (const tab of tabs) {
    await page.locator(`#settingsPanel [data-settings-tab="${tab}"]`).click();
    await expect(page.locator(`#settingsPanel [data-settings-tab="${tab}"]`)).toHaveAttribute("aria-selected", "true");
    heights[tab] = await page.locator("#settingsPanel .dialog-card").evaluate((el) => Math.round(el.getBoundingClientRect().height));
  }
  expect(new Set(Object.values(heights)).size, `Settings card heights per page: ${JSON.stringify(heights)}`).toBe(1);
});
