// Four defects in the pointer pipeline, all of them cases where a gesture was
// routed to the wrong handler rather than resolved to the wrong position. None
// corrupts content — the painted range is still the range that gets edited — but
// each one breaks an ordinary editing action outright.
import { test, expect, gotoEditor, clickIntoFirstPage, stableBox } from "./fixtures.mjs";

const selectedText = (page) =>
  page.evaluate(() => {
    const sel = document.querySelector("#a11ySelection");
    return sel ? sel.textContent.trim() : null;
  });

const highlightCount = (page) => page.locator(".overlay .highlight").count();

/** Opens the default header by double-clicking the top margin. */
async function editHeader(page) {
  const box = await stableBox(page.locator(".page-wrap .page").first());
  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 18);
  await expect(page.locator("#pages")).toHaveAttribute("data-running-edit", "header");
  return box;
}

// HF-142. Entering the band is what a double-click means from OUTSIDE it. Once
// you are already editing that band the gesture means what it means everywhere
// else, and re-entering the context you are already in selected nothing at all.
// Triple-click at the identical pixel always worked, which is what proved the
// hit-testing was fine and only the routing was not.
test("double-click selects a word inside the header being edited", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const box = await editHeader(page);

  // Type a known word so the assertion does not depend on the fixture's header.
  await page.keyboard.type("HEADERWORD");
  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 18);

  expect(await highlightCount(page), "nothing was selected").toBeGreaterThan(0);

  expect(consoleErrors).toEqual([]);
});

// HF-143 — the worst of the four. A range cannot span WordprocessingML stories,
// and shift-clicking out of an open header built exactly that: the anchor stayed
// in the header while the focus landed in the body. The result was 422 highlight
// rectangles across every page and an editor that then swallowed every keystroke,
// because no edit can apply to a range whose ends live in different stories.
// The visible half of HF-143 — 422 highlight rectangles across all 15 pages — is
// deliberately NOT asserted here. On this fixture the cross-story range paints
// nothing in the body at all, so both a rectangle count and a geometric bound
// pass against the unfixed code; proving it needs a multi-page document, and
// filling one from the keyboard costs more than it buys. What IS asserted below
// is the damage the user actually suffers, and it fails against the bug: the
// editor stops accepting input entirely.

test("the editor still accepts typing after shift-clicking out of a header", async ({
  page,
}) => {
  await gotoEditor(page);
  const box = await editHeader(page);
  await page.keyboard.type("H");

  await page.keyboard.down("Shift");
  await page.mouse.click(box.x + box.width * 0.4, box.y + box.height * 0.35);
  await page.keyboard.up("Shift");

  // The real damage was here: with a cross-story range the next keystroke was
  // rejected and the character count never moved.
  const before = await page.locator("#statChars").textContent();
  await page.keyboard.type("Z");
  await expect.poll(() => page.locator("#statChars").textContent()).not.toBe(before);
  await expect(page.locator("#status")).not.toContainText(/isn't supported/i);
});

// HF-144. The gutter was reserved from the presence of a change rather than from
// the mode, so the page slid sideways the moment a reviewer made their first
// edit — and slid back on Undo. Everything they were aiming at moved under the
// pointer, at the worst possible moment.
test("the first tracked change does not shift the page sideways", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('#reviewModeControl [data-review-mode="suggesting"]').click();

  const pageX = async () => (await stableBox(page.locator(".page-wrap").first())).x;
  const before = await pageX();
  await page.keyboard.type("x");
  await expect.poll(() => page.locator("#documentState").getAttribute("data-state")).toBe("edited");

  const after = await pageX();
  expect(
    Math.abs(after - before),
    `the page moved ${Math.round(Math.abs(after - before))}px when the first change landed`,
  ).toBeLessThanOrEqual(2);

  expect(consoleErrors).toEqual([]);
});

// HF-145. `startSelectionAutoScroll` computed only `dy`, so at any zoom where the
// sheet is wider than the window a drag-selection simply stopped at the window
// edge and the end of the line could not be reached with the mouse at all.
test("drag-selecting past the right edge scrolls horizontally", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Zoom until the sheet is genuinely wider than the viewport; without that
  // precondition there is nothing to scroll and the test would pass vacuously.
  for (let step = 0; step < 4; step++) await page.locator("#zoomIn").click();
  const overflowing = await page.evaluate(() => {
    const viewport = document.getElementById("viewport");
    return viewport.scrollWidth > viewport.clientWidth + 1;
  });
  expect(overflowing, "the page is not wider than the window; nothing to scroll").toBe(true);

  const view = await page.locator("#viewport").boundingBox();
  await page.mouse.move(view.x + 40, view.y + view.height * 0.4);
  await page.mouse.down();
  // Hold the pointer inside the auto-scroll edge band and let frames run.
  await page.mouse.move(view.x + view.width - 4, view.y + view.height * 0.4, { steps: 6 });
  await expect
    .poll(async () => page.evaluate(() => document.getElementById("viewport").scrollLeft), {
      timeout: 5000,
    })
    .toBeGreaterThan(0);
  await page.mouse.up();

  expect(consoleErrors).toEqual([]);
});
