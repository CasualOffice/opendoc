// Turning the formatting marks on is a REPAINT, and the ¶ button reads the
// engine (`docs/153` `view.formatting-marks`).
//
// `formatting_marks.mjs` has carried this sentence since it was written —
// "`view-formatting-marks.spec.mjs` asserts the page count is unchanged, which is
// the half a 'the marks appeared' test cannot see" — and the file did not exist.
// That is `SKILL` §9 rule 2 exactly: prose describing a gate, with nothing
// arming it. This is the gate.
//
// WHY IT HAS TO BE HERE AND NOT IN THE UNIT GUARD. The chrome's only view lever
// is the `repaint` thunk `main.js` injects:
//
//   repaint: () => {
//     for (let i = pageWindow.first; i <= pageWindow.last; i++) repaintPage(i);
//   },
//
// `repaintPage` drops and re-rasters ONE page's canvas and leaves its `.page-wrap`
// sheet alone — O(the materialized page window). `renderAll()` is the repagination:
// it reads `doc.pageCount`, loops every page, builds a fresh band element and
// `replaceChildren`s it into the scroller — O(document), and it throws away every
// sheet and the reader's scroll position with them. Writing `repaint: () => void
// renderAll()` is a one-word change that looks like a tidy-up, costs nothing on a
// six-page fixture, and makes the most-pressed button in the editor O(document) on
// the 1.3M-paragraph file (`SKILL` §8, "performance is a gate, not a hope").
//
// So what is asserted is the DIFFERENCE between those two, not the number of
// milliseconds either takes:
//
//   * the sheet elements are the SAME DOM NODES afterwards — probed with a
//     dataset marker before the gesture, which only survives a repaint;
//   * the reader's scroll position is where they left it;
//   * the page count is unchanged (the sentence the module already claimed);
//   * and the raster actually CHANGED, so this is not passing by doing nothing.
//
// MUTATION PROOF. `src/main.js`, the `createFormattingMarks` io block, with
// `repaint: () => void renderAll(),` in place of the loop over `pageWindow` —
// applied, run, restored. What it printed:
//
//   ✘  1 [chromium] › view-formatting-marks.spec.mjs:96:1 › turning the marks on
//        repaints the pages on screen and repaginates nothing (2.0s)
//
//     Error: the page sheets were rebuilt: this was a repagination, not a repaint
//     expect(received).toEqual(expected) // deep equality
//     Expected: 1
//     Received: 0
//
//   1 failed / 2 passed
//
// The other two passed, and that is the point rather than a weakness: the
// lying-toggle half and the repaint half are independent defects, so neither
// assertion can stand in for the other. It is also why `Expected: 1` — the band
// renderAll rebuilt re-materialized a sheet, it just was not the stamped one.
import {
  MOD,
  clickIntoFirstPage,
  documentPageCount,
  expect,
  gotoEditor,
  runPaletteCommand,
  test,
} from "./fixtures.mjs";

/** Stamps every sheet on screen, so a rebuilt band is detectable afterwards.
 *
 *  A COUNT of survivors, not a boolean: a repagination that happened to
 *  re-materialize the same number of sheets would still have lost the stamps, and
 *  a repaint cannot lose a single one. */
async function stampSheets(page) {
  return page.evaluate(() => {
    const sheets = [...document.querySelectorAll(".page-wrap")];
    sheets.forEach((sheet, i) => void (sheet.dataset.marksProbe = String(i)));
    return sheets.length;
  });
}

const survivingStamps = (page) =>
  page.evaluate(() => document.querySelectorAll(".page-wrap[data-marks-probe]").length);

/** A cheap fingerprint of the first page's raster, so "the marks appeared" is an
 *  assertion about pixels rather than about an attribute. The canvas is filled by
 *  `putImageData` from the engine's own RGBA, same-origin, so it is not tainted. */
async function firstPageRaster(page) {
  return page.evaluate(() => {
    const canvas = document.querySelector(".page-wrap canvas.page");
    if (!canvas) return null;
    // A column of samples rather than the whole data URL: enough to move when a
    // pilcrow, five space dots and a tab arrow are added to a page of text, and
    // small enough not to ship a megabyte through the CDP channel.
    const data = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
    let hash = 0;
    for (let i = 0; i < data.length; i += 997) hash = (hash * 31 + data[i]) | 0;
    return hash;
  });
}

const marksButton = (page) => page.locator("#formattingMarksBtn");

test("turning the marks on repaints the pages on screen and repaginates nothing", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Scrolled AWAY FROM THE TOP on purpose. At scrollTop 0 a repagination and a
  // repaint are indistinguishable by scroll position, which is how this exact
  // family of defect (`main.js`'s scroll-reset-on-re-render) kept shipping.
  await page.locator("#viewport").evaluate((el) => void (el.scrollTop = 240));
  await expect.poll(() => page.locator("#viewport").evaluate((el) => el.scrollTop)).toBe(240);

  const pagesBefore = await documentPageCount(page);
  const stamped = await stampSheets(page);
  expect(stamped, "no sheets were on screen to stamp").toBeGreaterThan(0);
  const rasterBefore = await firstPageRaster(page);
  expect(rasterBefore, "the first page had no raster to compare").not.toBeNull();

  await expect(marksButton(page)).toHaveAttribute("aria-pressed", "false");
  await marksButton(page).click();
  await expect(marksButton(page)).toHaveAttribute("aria-pressed", "true");

  // The repaint happened: the page looks different, because there are now marks
  // on it. Without this the three assertions below would all pass over a button
  // that did nothing at all.
  await expect
    .poll(() => firstPageRaster(page))
    .not.toBe(rasterBefore);

  expect(
    await survivingStamps(page),
    "the page sheets were rebuilt: this was a repagination, not a repaint",
  ).toEqual(stamped);
  expect(
    await page.locator("#viewport").evaluate((el) => el.scrollTop),
    "the reader's scroll position moved",
  ).toBe(240);
  expect(await documentPageCount(page), "a view change moved a page boundary").toBe(pagesBefore);
  expect(consoleErrors).toEqual([]);
});

test("the pressed state is the engine's, and one switch moves one mark", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // One mark, from the popover. The ¶ button must then read PRESSED, because its
  // state is the engine's `any` — a control holding its own "did I press Show
  // All" boolean reads false here, which is the lying toggle.
  await page.locator("#formattingMarksMenuBtn").click();
  await expect(page.locator("#formattingMarksMenu")).toBeVisible();
  const row = (mark) => page.locator(`#formattingMarksMenu [data-mark="${mark}"]`);
  await row("space").click();
  await expect(row("space")).toHaveAttribute("aria-checked", "true");
  await expect(marksButton(page)).toHaveAttribute("aria-pressed", "true");
  // And the other four are untouched: the setter takes a PATCH so that one row is
  // one mark.
  for (const other of ["tab", "paragraph", "lineBreak", "pageBreak"]) {
    await expect(row(other), other).toHaveAttribute("aria-checked", "false");
  }

  // The popover STAYS OPEN — five switches a reader flips in a row, which is
  // Word's Display pane and ONLYOFFICE's own split menu.
  await expect(page.locator("#formattingMarksMenu")).toBeVisible();
  await row("tab").click();
  await expect(row("tab")).toHaveAttribute("aria-checked", "true");
  await expect(row("space"), "turning the tab arrows on switched the space dots off").toHaveAttribute(
    "aria-checked",
    "true",
  );
  await page.keyboard.press("Escape");

  // ¶ from here is OFF, not "fill in the other three": Word's button shows all or
  // nothing and `any` decides the direction.
  await marksButton(page).click();
  await expect(marksButton(page)).toHaveAttribute("aria-pressed", "false");
  expect(consoleErrors).toEqual([]);
});

test("the same gesture is reachable from the palette, and reports its state there", async ({
  page,
  consoleErrors,
}) => {
  // `SKILL` §10: every capability from two surfaces or more. The ribbon's split
  // button is Word's placement, View ▸ Formatting marks is where a reader trained
  // on Docs looks, and the palette is the third.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await runPaletteCommand(page, "view.formattingMarks", "formatting marks");
  await expect(marksButton(page)).toHaveAttribute("aria-pressed", "true");
  // The row's own label carries the state back, so a reader sees it without
  // opening the ribbon.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("formatting marks");
  await expect(
    page.locator('#cmdList .cmd-item[data-command-id="view.formattingMarks"]'),
    // The LABEL, not the whole row: the row also renders the chord, and `⌘8`
    // becomes `Ctrl+8` on the Linux runner (`SKILL` §11).
  ).toContainText(/marks: on/);
  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});
