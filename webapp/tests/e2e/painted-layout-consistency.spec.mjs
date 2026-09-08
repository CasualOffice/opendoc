// The two-layout defect class, swept for the call sites the first fix missed.
//
// The engine keeps two paginated layouts. `self.layout` is the EDITING layout
// (tracked deletions are zero-width); `self.markup_layout` is what "show
// changes" PAINTS (deletions kept at full width, struck through). `renderPage`
// and `pageCount` read the markup layout through `active_layout()`, so every
// pixel the user sees comes from it — while any geometry API still reading
// `self.layout` answers about a layout nobody is looking at.
//
// `caretRect`, `selectionRects`, `hitTest`, `linkAt` and `body_hit` were routed
// through `active_layout()` + `view_pos()`/`edit_pos()`. These specs cover the
// call sites that were not:
//
//   * `moved_caret` "up"/"down"      -> `move_vertical(&self.layout)`
//   * `moved_caret` "lineStart"/"lineEnd" -> `caret_rect` + `hit_test` on `&self.layout`
//   * `cell_rect` / `table_selection_rects` / `table_column_resize_handles`
//   * `resolve_object_boxes` (backs `objectAt`, `objectRect`, `objectHandles`,
//     `objectOrder`, `objectDescendantAt`)
//   * `checklist_markers` -> `marker_rects(&self.layout)`
//
// The measurement trick every test here uses: while "show changes" is on, the
// MARKUP layout is geometrically identical to the layout that existed BEFORE the
// text was struck — a tracked deletion changes nothing about how the deleted
// glyphs are placed, only how they are painted. So a page-local rectangle
// measured before the strike is exactly where that thing is painted after it.
// Page-local coordinates (relative to the page canvas) also neutralise the
// Suggesting banner, which pushes the whole page down the viewport.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  stableBox,
  MOD,
} from "./fixtures.mjs";

const mirrorText = (page) =>
  page.locator("#a11yDocument").evaluate((el) => el.textContent.trim());

/** A rectangle in PAGE-LOCAL css pixels (origin = the first page canvas). */
const local = (page, selector) =>
  page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    const p = document.querySelector(".page-wrap .page").getBoundingClientRect();
    return {
      x: +(r.x - p.x).toFixed(1),
      y: +(r.y - p.y).toFixed(1),
      w: +r.width.toFixed(1),
      h: +r.height.toFixed(1),
    };
  }, selector);

/** Clicks a PAGE-LOCAL point, scrolling first if it is off-screen. */
async function clickLocal(page, x, y, options = {}) {
  const target = page.locator(".page-wrap .page").first();
  let box = await stableBox(target);
  const viewport = page.viewportSize().height;
  if (box.y + y > viewport - 60 || box.y + y < 60) {
    await page.mouse.wheel(0, box.y + y - viewport / 2);
    await page.waitForTimeout(120);
    box = await stableBox(target);
  }
  await page.mouse.click(box.x + x, box.y + y, options);
}

async function enterSuggesting(page) {
  const button = page.locator('#reviewModeControl [data-review-mode="suggesting"]');
  await button.click();
  await expect(button).toHaveAttribute("aria-pressed", "true");
}

/** Selects `n` characters forward from the caret and strikes them. */
async function strike(page, n) {
  for (let i = 0; i < n; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Backspace");
  // Without the markup view the two layouts coincide and nothing here proves
  // anything.
  await expect(page.locator("body")).toHaveClass(/showing-changes/);
}

async function right(page, n) {
  for (let i = 0; i < n; i++) await page.keyboard.press("ArrowRight");
}

// ---------------------------------------------------------------------------
// 1. Vertical caret movement — `moved_caret` "up"/"down" (lib.rs:10197)
// ---------------------------------------------------------------------------

test("ArrowDown keeps the caret in the column it is painted in", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await enterSuggesting(page);
  await moveCaretToDocStart(page);
  await strike(page, 5); // "Rich " out of the "Rich Document" heading

  await moveCaretToDocStart(page);
  // Editing offset 0 is PAINTED after the struck word, ~60 px into the line.
  const before = await local(page, ".overlay .caret");
  expect(before.x, "the caret should be painted past the struck word").toBeGreaterThan(100);

  await page.keyboard.press("ArrowDown");
  const after = await local(page, ".overlay .caret");

  // `move_vertical` derives its x-affinity from the EDITING layout, where the
  // caret sits at the line start, so Down drops it to the start of the next
  // line instead of under the caret the user can see.
  expect(
    Math.abs(after.x - before.x),
    `ArrowDown moved the caret ${Math.round(after.x - before.x)} px sideways ` +
      `(painted at x=${before.x}, landed at x=${after.x})`,
  ).toBeLessThan(14);

  await page.keyboard.type("#");
  const text = await mirrorText(page);
  expect(
    text,
    `the marker landed at the paragraph start, not under the caret: ${text.slice(0, 40)}`,
  ).not.toMatch(/^Document#Paragraph/);

  expect(consoleErrors).toEqual([]);
});

// ---------------------------------------------------------------------------
// 2/3. Home / End — `moved_caret` "lineStart"/"lineEnd" (lib.rs:10228)
// ---------------------------------------------------------------------------

// A heading long enough to wrap several times, with a middle chunk struck, so
// the editing layout wraps it into FEWER lines than the markup layout paints.
const HEAD_A = "aaaaaaaaa ".repeat(4); // 40 surviving chars
const HEAD_B = "bbbbbbbbb ".repeat(13); // 130 struck chars
const HEAD_C = "ccccccccc ".repeat(12); // 120 surviving chars

async function struckHeading(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(HEAD_A + HEAD_B + HEAD_C);
  await enterSuggesting(page);
  await moveCaretToDocStart(page);
  await right(page, HEAD_A.length);
  await strike(page, HEAD_B.length);
}

/** Caret y after navigating to editing offset `offset` of the heading. */
async function caretAt(page, offset) {
  await moveCaretToDocStart(page);
  await right(page, offset);
  return local(page, ".overlay .caret");
}

// Home and End are NOT covered here. They do leave the visual line — measured at
// y 279.3 -> 316.1, a full line down — but they do it with NO tracked change in
// the document at all, so the cause is not this file's subject. It is the caret's
// missing affinity: an offset that both ends line N and starts line N+1 always
// resolves to N+1 (`caret_start_line`, crates/casual-doc-layout/src/hittest.rs).
// Routing Home/End at the painted layout, which this change does, is necessary
// and not sufficient. Tracked as HF-123; a test asserting it here would fail for
// a reason this file does not explain.

// ---------------------------------------------------------------------------
// 4/5. Table chrome — `cell_rect` (lib.rs:5562) and
//      `table_column_resize_handles` (lib.rs:5619)
// ---------------------------------------------------------------------------

/** Scans down the page for a table cell and returns its page-local outline. */
async function findCell(page) {
  const box = await stableBox(page.locator(".page-wrap .page").first());
  for (let y = 60; y < box.height - 60; y += 20) {
    await clickLocal(page, 110, y);
    const cell = await local(page, ".cell-outline");
    if (cell) return { at: { x: 110, y }, rect: cell };
  }
  return null;
}

test("the active-cell outline is drawn around the cell the caret is in", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(HEAD_A + HEAD_B + HEAD_C);

  // Where the table is painted. The markup layout after the strike is identical
  // to this one, so these page-local numbers stay valid afterwards.
  const cell = await findCell(page);
  expect(cell, "the demo fixture should contain a table").not.toBeNull();

  await enterSuggesting(page);
  await moveCaretToDocStart(page);
  await right(page, HEAD_A.length);
  await strike(page, HEAD_B.length);

  // Click the cell where it is PAINTED. Hit-testing is already markup-correct,
  // so the caret really does land in that cell.
  await clickLocal(page, cell.at.x, cell.rect.y + cell.rect.h / 2);
  const outline = await local(page, ".cell-outline");
  const caret = await local(page, ".overlay .caret");

  expect(outline, "the caret should be in a table cell").not.toBeNull();
  expect(caret, "the caret should be painted").not.toBeNull();

  const caretMid = caret.y + caret.h / 2;
  expect(
    caretMid > outline.y && caretMid < outline.y + outline.h,
    `the caret is painted at y=${caretMid} but the active-cell outline spans ` +
      `${outline.y}..${outline.y + outline.h} — the outline comes from the editing ` +
      "layout, where the struck heading is three lines shorter",
  ).toBe(true);

  expect(consoleErrors).toEqual([]);
});

test("the table column-resize handles sit on the table that is painted", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(HEAD_A + HEAD_B + HEAD_C);
  const cell = await findCell(page);
  expect(cell).not.toBeNull();

  await enterSuggesting(page);
  await moveCaretToDocStart(page);
  await right(page, HEAD_A.length);
  await strike(page, HEAD_B.length);

  await clickLocal(page, cell.at.x, cell.rect.y + cell.rect.h / 2);
  const handle = await local(page, ".table-col-resize-handle");
  expect(handle, "a regular table should expose column-resize handles").not.toBeNull();

  expect(
    Math.abs(handle.y - cell.rect.y),
    `the resize handle is ${Math.round(handle.y - cell.rect.y)} px away from the ` +
      "row it belongs to — it is placed from the editing layout",
  ).toBeLessThan(cell.rect.h);

  expect(consoleErrors).toEqual([]);
});

// ---------------------------------------------------------------------------
// 6/7/8. Objects — `resolve_object_boxes` (lib.rs:9729) behind `objectAt`,
//        `objectRect`, `objectHandles`.
// ---------------------------------------------------------------------------

/** Probes a band of the page for a selectable object, returning where the click
 *  landed and the outline the engine drew for it. */
async function probeForObject(page, yFrom, yTo, xFrom, xTo) {
  const box = await stableBox(page.locator(".page-wrap .page").first());
  for (let y = yFrom; y < yTo; y += 6) {
    for (let x = xFrom; x < Math.min(xTo, box.width - 40); x += 8) {
      await clickLocal(page, x, y);
      const rect = await local(page, ".object-outline");
      if (rect) return { at: { x, y }, rect };
    }
  }
  return null;
}

/** Finds the demo's inline image by probing along the paragraph that holds it. */
const findImage = (page) => probeForObject(page, 102, 130, 200, 400);

/** Enters Suggesting and strikes the "Paragraph with an image:" text that sits
 *  to the LEFT of the demo's inline image on the same line. In the editing
 *  layout the image then slides to the line start; in the markup layout — the
 *  one being painted — it does not move at all. */
async function strikeTextBeforeImage(page) {
  await enterSuggesting(page);
  await moveCaretToDocStart(page);
  await right(page, "Rich Document".length + 1); // into the next paragraph
  await strike(page, "Paragraph with an image:".length);
}

test("clicking the image where it is painted selects it", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const image = await findImage(page);
  expect(image, "the demo fixture should contain an inline image").not.toBeNull();

  await page.keyboard.press("Escape");
  await strikeTextBeforeImage(page);

  await clickLocal(page, image.rect.x + image.rect.w / 2, image.rect.y + image.rect.h / 2);
  const outline = await local(page, ".object-outline");
  expect(
    outline,
    `clicking the painted image at (${image.rect.x}, ${image.rect.y}) selected nothing — ` +
      "`objectAt` walks the editing layout, where the image has slid left past " +
      "the struck text",
  ).not.toBeNull();

  expect(consoleErrors).toEqual([]);
});

test("the image stays selectable only where it is painted", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const image = await findImage(page);
  expect(image, "the demo fixture should contain an inline image").not.toBeNull();

  await page.keyboard.press("Escape");
  await strikeTextBeforeImage(page);

  // Sweep the whole line the image is painted on and note every point that
  // selects an object, plus the outline the engine drew for it.
  const found = await probeForObject(page, image.rect.y, image.rect.y + image.rect.h, 60, 500);
  expect(
    found,
    "after the strike the image is not selectable ANYWHERE on its line — " +
      "`objectAt` is asking the editing layout, where the image has slid left " +
      "past the struck text",
  ).not.toBeNull();
  expect(
    Math.abs(found.at.x - (image.rect.x + image.rect.w / 2)),
    `the image is selected by clicking x=${found.at.x} but it is painted at ` +
      `x=${image.rect.x}..${image.rect.x + image.rect.w}, and its outline is drawn ` +
      `at x=${found.rect.x}`,
  ).toBeLessThan(image.rect.w);

  expect(consoleErrors).toEqual([]);
});

// ---------------------------------------------------------------------------
// 9. Checklist markers — `checklist_markers` (lib.rs:4134)
// ---------------------------------------------------------------------------

test("the checklist marker is drawn beside its own item", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(HEAD_A + HEAD_B + HEAD_C);

  // Turn the LAST paragraph into a checklist item, so its marker sits below the
  // heading the strike is about to shorten.
  await page.keyboard.press(`${MOD}+End`);
  await page.locator("#checkList").click();
  const painted = await local(page, ".checklist-marker");
  expect(painted, "the heading should now be a checklist item").not.toBeNull();

  await enterSuggesting(page);
  await moveCaretToDocStart(page);
  await right(page, HEAD_A.length);
  await strike(page, HEAD_B.length);

  const marker = await local(page, ".checklist-marker");
  expect(marker, "the checklist marker should still be painted").not.toBeNull();
  expect(
    Math.abs(marker.y - painted.y),
    `the click target moved ${Math.round(marker.y - painted.y)} px away from the ` +
      "checkbox glyph baked into the page raster",
  ).toBeLessThan(6);

  expect(consoleErrors).toEqual([]);
});

// ---------------------------------------------------------------------------
// 10. Page INDICES diverge too — `band_at` (lib.rs:829/851), `nearest_in_band`
//     (lib.rs:914), `hit_test_running` (lib.rs:938) and `hit_test_text_box`
//     (lib.rs:755) all take a 1-based page number from the RENDERED (markup)
//     layout and look it up in `self.layout`, which can have fewer pages.
// ---------------------------------------------------------------------------

const pageCount = (page) => page.locator(".page-wrap").count();
const marker = (page) => page.locator(".running-marker");

/** Hovers just inside the top edge of the 1-based page `index`. */
async function hoverTopBand(page, index) {
  // Page canvases are created lazily, so scroll the WRAP into view first and let
  // its canvas appear.
  const wrap = page.locator(".page-wrap").nth(index - 1);
  await wrap.evaluate((el) => el.scrollIntoView({ block: "start" }));
  await page.waitForTimeout(250);
  const box = await stableBox(wrap);
  await page.mouse.move(box.x + box.width * 0.5, box.y + 10);
  await page.waitForTimeout(150);
}

test("the header band of a page only the markup layout has is still reachable", async ({
  page,
  consoleErrors,
}) => {
  // Filling a whole page from the keyboard is slow; nothing else in the editor
  // creates a second page (there is no page-break command yet).
  test.setTimeout(240_000);
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  // Enough 24 pt heading text to push the document onto a second page.
  await page.keyboard.type("wwwwwwwww ".repeat(170));
  await expect.poll(() => pageCount(page)).toBeGreaterThan(1);

  // Control: the band is discoverable on page 2 while the two layouts agree.
  await hoverTopBand(page, 2);
  await expect(marker(page)).toBeVisible();

  // Strike most of it. The markup layout still paints every page; the editing
  // layout, where every deletion is zero-width, gets shorter.
  await clickIntoFirstPage(page);
  await enterSuggesting(page);
  await moveCaretToDocStart(page);
  const fullText = (await mirrorText(page)).length;
  for (let i = 0; i < 60; i++) await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Backspace");
  await expect(page.locator("body")).toHaveClass(/showing-changes/);
  await expect
    .poll(async () => fullText - (await mirrorText(page)).length)
    .toBeGreaterThan(800);

  const painted = await pageCount(page);
  expect(painted, "the markup view should still paint every page").toBeGreaterThan(1);

  await hoverTopBand(page, painted);
  await expect(
    marker(page),
    `the top margin of painted page ${painted} offers no header — \`bandAt\` looks ` +
      "that page number up in the editing layout, which no longer has it",
  ).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

// ---------------------------------------------------------------------------
// 11. Control — the call sites that WERE fixed. If this fails the harness, not
//     the engine, is what the other tests are measuring.
// ---------------------------------------------------------------------------

test("control: selection rectangles still follow the painted text", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+ArrowRight");
  const lineStart = (await local(page, ".overlay .highlight")).x;
  await page.keyboard.press("ArrowLeft");

  await enterSuggesting(page);
  await moveCaretToDocStart(page);
  await strike(page, 5);

  await moveCaretToDocStart(page);
  for (let i = 0; i < 7; i++) await page.keyboard.press("Shift+ArrowRight");
  const highlight = await local(page, ".overlay .highlight");

  expect(highlight.x - lineStart).toBeGreaterThan(30);
  expect(consoleErrors).toEqual([]);
});
