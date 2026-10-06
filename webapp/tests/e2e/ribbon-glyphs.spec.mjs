// Nothing in the chrome paints a cut glyph, and nothing paints outside the
// control that holds it.
//
// Two defects, one class (`109` UX-041, UX-042):
//
//   * A STRAY "G" in the bottom-right corner of the Review band. The grammar
//     toggle carried a `.table-command-badge` — `position: absolute` — and only
//     `.table-ribbon .fmt` was `position: relative`, so outside the Table band the
//     badge resolved against `.ribbon-body` and painted ~600px from its button.
//     Every existing guard was green: the button was the right size, in the right
//     place, in the band. Only the geometry of what is INSIDE it says anything.
//   * CLIPPED CARETS. Every `.ms` is `overflow: hidden`, and a flex item that
//     clips has an automatic minimum width of zero — so the zoom-presets caret,
//     with the user agent's 6px of button padding a side, shrank to 6 of its 16px
//     and drew as a dot, and the split-button carets on Home to 12 or 15 of 18.
//
// The assertions are the guarantee rather than the mechanism: every glyph box is
// at least as wide as its glyph, and every painted thing inside a control lies
// inside that control's border box. They are modelled on `dialog-contract.spec`'s
// `measureClipping`, which asks the same of every modal.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

const TABS = ["home", "insert", "layout", "references", "review", "view"];

/** Every glyph squeezed below its own width, and every painted descendant of a
 *  control that escapes the control's box, inside `rootSelector`. Serialised into
 *  the page, so it closes over nothing. */
const measure = (rootSelector) => {
  const out = [];
  const name = (el) =>
    (el.id && `#${el.id}`) ||
    `${el.tagName.toLowerCase()}.${String(el.className).trim().split(/\s+/)[0] || ""}`;
  for (const root of document.querySelectorAll(rootSelector)) {
    if (root.getClientRects().length === 0) continue;
    for (const icon of root.querySelectorAll(".ms")) {
      if (icon.getClientRects().length === 0) continue;
      if (icon.scrollWidth > icon.clientWidth + 1) {
        out.push(
          `${name(icon.parentElement)} > .ms "${icon.textContent}": glyph is ${icon.scrollWidth}px wide in a ${icon.clientWidth}px box`,
        );
      }
    }
    for (const control of root.querySelectorAll("button, input, select, .access-badge")) {
      if (control.getClientRects().length === 0) continue;
      const box = control.getBoundingClientRect();
      for (const child of control.querySelectorAll("*")) {
        if (child.getClientRects().length === 0) continue;
        const r = child.getBoundingClientRect();
        if (r.width === 0 && r.height === 0) continue;
        if (
          r.left < box.left - 1 ||
          r.right > box.right + 1 ||
          r.top < box.top - 1 ||
          r.bottom > box.bottom + 1
        ) {
          out.push(
            `${name(child)} "${child.textContent.trim().slice(0, 20)}" paints at ` +
              `${Math.round(r.left)},${Math.round(r.top)}-${Math.round(r.right)},${Math.round(r.bottom)} ` +
              `outside ${name(control)} at ${Math.round(box.left)},${Math.round(box.top)}-` +
              `${Math.round(box.right)},${Math.round(box.bottom)}`,
          );
        }
      }
    }
  }
  return out;
};

test("no glyph in any ribbon band is cut, and nothing paints outside its control", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const found = [];
  for (const tab of TABS) {
    await page.locator(`[data-tab="${tab}"]`).click();
    await expect(page.locator(`.ribbon-panel[data-panel="${tab}"]`)).toBeVisible();
    for (const line of await page.evaluate(measure, `.ribbon-panel[data-panel="${tab}"]`)) {
      found.push(`${tab}: ${line}`);
    }
  }
  expect(found, `the ribbon paints cut or escaped glyphs:\n  ${found.join("\n  ")}`).toEqual([]);
});

test("the Review band has no stray badge: Grammar is told apart by its label and glyph", async ({
  page,
}) => {
  // The specific instance, pinned beside the class: the two proofing toggles used
  // to share the `spellcheck` glyph and be told apart only by a 7px "G" — the very
  // badge that escaped. Now each carries its own word and its own glyph.
  await gotoEditor(page);
  await page.locator("#tabReview").click();
  await expect(page.locator("#reviewGrammarCheckBtn .table-command-badge")).toHaveCount(0);
  await expect(page.locator("#reviewSpellCheckBtn .fmt-big-label")).toHaveText("Spelling");
  await expect(page.locator("#reviewGrammarCheckBtn .fmt-big-label")).toHaveText("Grammar");
  const glyphs = await page.$$eval(
    "#reviewSpellCheckBtn .ms, #reviewGrammarCheckBtn .ms",
    (els) => els.map((el) => el.textContent),
  );
  expect(new Set(glyphs).size, `both toggles draw ${glyphs[0]}`).toBe(2);
});

test("no glyph in the status bar or the header is cut, and nothing paints outside its control", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  const found = await page.evaluate(measure, ".footer, header.bar");
  expect(found, `the status bar or header paints cut glyphs:\n  ${found.join("\n  ")}`).toEqual([]);
});

test("the zoom-presets caret is drawn whole, not as a dot", async ({ page }) => {
  // The instance the owner saw: 6 of 16px. Asserted as a WIDTH as well as by the
  // general rule above, because "the caret is visible" is the user's guarantee
  // and a glyph box narrower than its em is exactly the dot.
  await gotoEditor(page);
  const { box, em } = await page.$eval("#zoomMenuBtn .ms", (el) => ({
    box: el.getBoundingClientRect().width,
    em: parseFloat(getComputedStyle(el).fontSize),
  }));
  expect(box).toBeGreaterThanOrEqual(em - 0.5);
});
