// The section and header/footer properties, end to end.
//
// Five `w:sectPr` properties the engine has imported, laid out, painted and
// round-tripped for months with NOTHING in the product able to change one of them
// (`docs/99` §9.4): header distance from top, footer distance from bottom, gutter,
// vertical alignment, and page numbering (format and start).
//
// So the load-bearing assertion in every test here is INK MOVING ON THE PAGE, not
// a field reading back what was typed into it. A spec that asserted
// `sectionLayout()` returns `verticalAlignment: "bottom"` would pass against a UI
// that writes the property and a renderer that ignores it — and for four of these
// five the property was ALREADY readable and writable from the engine's side
// before this round, so host state is exactly the thing that proves nothing.
// `line-numbers.spec.mjs` records the same trap for the same reason.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD, documentPageCount, pageSheet } from "./fixtures.mjs";

/** Dark-pixel statistics for a normalized region of one page's raster.
 *
 *  `centroidY` is where the ink SITS vertically inside the region and `minX` is
 *  its leading edge — the two things a band distance, a vertical alignment and a
 *  gutter respectively move. `count` is how MUCH ink there is, which is what a
 *  page-number format changes ("1" against "VIII").
 *
 *  Waiting on `document.fonts.ready` is what makes this a measurement rather than
 *  a coin toss: without it a fallback face paints first and its glyphs are a
 *  different width, so the same unchanged page measures differently run to run
 *  (the figure `line-numbers.spec.mjs` records is 234, then 20, then 25).
 *
 *  Every assertion below is a RELATION between two measurements of the same
 *  document, never a pinned number: a pinned figure reddens `main` when a font or
 *  a metric changes without anything being removed. */
async function inkOf(page, pageNumber, region = { top: 0, height: 1, left: 0, width: 1 }) {
  await pageSheet(page, pageNumber);
  await page.evaluate(() => document.fonts.ready);
  return page.evaluate(
    ({ n, r }) => {
      const wrap = document.querySelector(`#pages .page-wrap[data-page-number="${n}"]`);
      const canvas = wrap.querySelector("canvas.page");
      const x = Math.round(canvas.width * r.left);
      const y = Math.round(canvas.height * r.top);
      const w = Math.max(1, Math.round(canvas.width * r.width));
      const h = Math.max(1, Math.round(canvas.height * r.height));
      const { data } = canvas.getContext("2d").getImageData(x, y, w, h);
      let count = 0;
      let sumY = 0;
      let minX = Infinity;
      for (let p = 0; p < data.length; p += 4) {
        if ((data[p] + data[p + 1] + data[p + 2]) / 3 < 160) {
          const i = p / 4;
          count += 1;
          sumY += Math.floor(i / w);
          minX = Math.min(minX, i % w);
        }
      }
      return {
        count,
        centroidY: count ? sumY / count : null,
        minX: Number.isFinite(minX) ? minX : null,
        width: w,
        height: h,
      };
    },
    { n: pageNumber, r: region },
  );
}

/** The top 15% of a page — the strip a header is painted into on a 1in-margin,
 *  11in sheet, with room for the band to move down without leaving the region. */
const HEADER_STRIP = { top: 0, height: 0.15, left: 0, width: 1 };

async function openHeaderFooterSettings(page) {
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#headerFooterSettingsBtn").click();
  await expect(page.locator("#headerFooterSettingsDialog")).toBeVisible();
}

async function openPageSetup(page) {
  await page.locator('[data-tab="layout"]').click();
  await page.locator("#layoutMarginsBtn").click();
  await expect(page.locator("#pageSetupMenu")).toBeVisible();
}

/** Sets one inches field and commits the dialog. `fill` rather than `type`,
 *  because `keyboard.type` is `preventDefault`ed for printable characters on this
 *  surface — a spec in this repo once drove a whole feature through an element
 *  that never received input for exactly that reason. */
async function applyHeaderFooter(page, values) {
  for (const [selector, value] of Object.entries(values)) {
    await page.locator(selector).fill(String(value));
  }
  await page.locator("#headerFooterSettingsApply").click();
  await expect(page.locator("#headerFooterSettingsDialog")).toBeHidden();
}

/** The multi-section fixture: four portrait pages owning "PORTRAIT HEADER", then
 *  two landscape pages with their own header and 2in vertical margins. It is the
 *  document that makes "which section is the caret in" a real question. */
async function gotoSections(page) {
  await page.goto("/editor.html?fixture=sections");
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status !== null &&
        status.textContent === "" &&
        document.querySelectorAll(".page-wrap").length > 0 &&
        document.body.dataset.fontsReady === "true"
      );
    },
    null,
    { timeout: 45_000 },
  );
  expect(await documentPageCount(page)).toBe(6);
}

/** A one-page document with nothing on it, so the only ink is what the test puts
 *  there. Vertical alignment needs a page with room left to distribute, and a
 *  page-number field's glyphs need to be the only thing on the raster for their
 *  ink to be countable. */
async function newBlankDocument(page) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  const row = page.locator('#cmdList .cmd-item[data-command-id="file.new"]');
  await expect(row).toBeVisible();
  await row.click();
  await expect(page.locator("#cmdPalette")).toBeHidden();
  await expect.poll(() => page.locator(".page-wrap").count()).toBe(1);
}

test("the header distance moves the header down the page, for the caret's section only, and one undo puts it back", async ({
  page,
  consoleErrors,
}) => {
  await gotoSections(page);

  // Page 1 is the portrait section's; page 5 is the landscape section's first.
  // `w:pgMar/@w:header` is per SECTION, so a change aimed at the page in front of
  // the user must not move a page they are not looking at — the docs/104 T-01
  // class, and the reason `pageSetupSections` answers "which section" exactly once.
  await clickIntoFirstPage(page);
  await openHeaderFooterSettings(page);

  // The dialog knows there are two sections and is showing the caret's.
  const sections = page.locator("#headerFooterSection option");
  await expect(sections).toHaveCount(2);
  await expect(page.locator("#headerFooterSection")).toHaveValue(
    await sections.first().getAttribute("value"),
  );

  await applyHeaderFooter(page, { "#headerFromTop": "0.15" });
  const high = await inkOf(page, 1, HEADER_STRIP);
  const landscapeBefore = await inkOf(page, 5, HEADER_STRIP);
  expect(high.count, "the portrait section has a header to move").toBeGreaterThan(0);

  await openHeaderFooterSettings(page);
  await applyHeaderFooter(page, { "#headerFromTop": "0.75" });
  const low = await inkOf(page, 1, HEADER_STRIP);

  // 0.6in further from the top edge, so the header's ink sits lower in the strip.
  // A relation, not a pixel count: the distance reached layout and the paint.
  expect(
    low.centroidY,
    `header ink centroid moved from ${high.centroidY} to ${low.centroidY} in a ` +
      `${low.height}px strip when the distance went from 0.15in to 0.75in`,
  ).toBeGreaterThan(high.centroidY + 1);

  // And the other section's header did not move.
  const landscapeAfter = await inkOf(page, 5, HEADER_STRIP);
  expect(
    landscapeAfter.centroidY,
    "the landscape section's header belongs to a section nobody edited",
  ).toBeCloseTo(landscapeBefore.centroidY, 0);

  // ONE undo, because one Apply is one action.
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(async () => (await inkOf(page, 1, HEADER_STRIP)).centroidY).toBeCloseTo(
    high.centroidY,
    0,
  );

  expect(consoleErrors).toEqual([]);
});

test("the footer distance moves the footer up from the bottom edge", async ({
  page,
  consoleErrors,
}) => {
  await gotoSections(page);
  await clickIntoFirstPage(page);

  const strip = { top: 0.85, height: 0.15, left: 0, width: 1 };
  await openHeaderFooterSettings(page);
  await applyHeaderFooter(page, { "#footerFromBottom": "0.15" });
  const low = await inkOf(page, 1, strip);
  expect(low.count, "the portrait section has a footer to move").toBeGreaterThan(0);

  await openHeaderFooterSettings(page);
  await applyHeaderFooter(page, { "#footerFromBottom": "0.75" });
  const high = await inkOf(page, 1, strip);

  // Further from the BOTTOM edge is higher up the page, so the centroid falls.
  expect(
    high.centroidY,
    `footer ink centroid moved from ${low.centroidY} to ${high.centroidY} when the ` +
      `distance from the bottom went from 0.15in to 0.75in`,
  ).toBeLessThan(low.centroidY - 1);

  expect(consoleErrors).toEqual([]);
});

test("vertical alignment moves a short page's content down, and undo brings it back", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await newBlankDocument(page);
  await page.keyboard.type("ALIGN");

  const top = await inkOf(page, 1);
  expect(top.count, "the typed word is on the page").toBeGreaterThan(0);

  await openPageSetup(page);
  await expect(page.locator("#pageVerticalAlignment")).toHaveValue("top");
  await page.locator("#pageVerticalAlignment").selectOption("bottom");
  await page.locator("#pageSetupApply").click();
  await expect(page.locator("#pageSetupMenu")).toBeHidden();

  const bottom = await inkOf(page, 1);
  // A nearly empty page has almost a whole sheet of room to distribute, so this is
  // not a subtle shift: the word crosses the middle of the page.
  expect(
    bottom.centroidY,
    `the word's centroid moved from ${top.centroidY} to ${bottom.centroidY} of ` +
      `${bottom.height}px when the section was aligned to the bottom`,
  ).toBeGreaterThan(bottom.height / 2);

  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(async () => (await inkOf(page, 1)).centroidY).toBeLessThan(top.centroidY + 2);

  expect(consoleErrors).toEqual([]);
});

test("changing only the vertical alignment costs ONE undo, so the next one reaches the typing", async ({
  page,
  consoleErrors,
}) => {
  // The guarantee is "one Apply is one undoable action", and the only way to assert
  // it on INK is to look at what the SECOND undo reaches. Page setup writes two
  // property families through two engine calls, and the engine pushes a history
  // entry for every call it is given — so an Apply that issued `setPageSetup`
  // unconditionally made a vertical-alignment change cost two presses, the second
  // of which reversed a geometry write that changed nothing. A test that only
  // pressed undo once would have stayed green through all of it, which is exactly
  // what happened: the sibling alignment test above does press undo once, and it
  // passed while this was broken.
  await gotoEditor(page);
  await newBlankDocument(page);
  await page.keyboard.type("ALIGN");

  const typed = await inkOf(page, 1);
  expect(typed.count, "the typed word is on the page").toBeGreaterThan(0);

  await openPageSetup(page);
  await expect(page.locator("#pageVerticalAlignment")).toHaveValue("top");
  await page.locator("#pageVerticalAlignment").selectOption("bottom");
  // Nothing else is touched, which is the whole point: the geometry half of this
  // dialog has no change to write, so it must not write one.
  await page.locator("#pageSetupApply").click();
  await expect(page.locator("#pageSetupMenu")).toBeHidden();

  const aligned = await inkOf(page, 1);
  expect(aligned.centroidY, "the alignment took effect").toBeGreaterThan(aligned.height / 2);

  // One undo reverses the alignment...
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(async () => (await inkOf(page, 1)).centroidY).toBeLessThan(typed.centroidY + 2);

  // ...and the NEXT one reaches the typing, because the Apply spent exactly one
  // entry. If it spent two, this undo reverses the second one and the word stays.
  await page.keyboard.press(`${MOD}+z`);
  await expect
    .poll(async () => (await inkOf(page, 1)).count, {
      message:
        "the second undo must reach the typing — a leftover no-op geometry entry " +
        "absorbs it and the word stays on the page",
    })
    .toBe(0);

  expect(consoleErrors).toEqual([]);
});

test("the page-number format and start reach the field the engine paints", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await newBlankDocument(page);

  // A PAGE field, which is what a page-number format is ABOUT. Inserted from the
  // Insert band's own button, so this also exercises the surface a user would use.
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertPageNumberBtn").click();
  const one = await inkOf(page, 1);
  expect(one.count, "the field paints a page number").toBeGreaterThan(0);

  // "1" becomes "888": three glyphs where there was one, so the ink grows. This is
  // the assertion that `w:pgNumType/@w:start` reaches `page_number_labels_for` and
  // not merely the model.
  await openHeaderFooterSettings(page);
  await page.locator("#pageNumberStart").fill("888");
  await expect(page.locator("#pageNumberRestart")).toBeChecked();
  await page.locator("#headerFooterSettingsApply").click();
  await expect(page.locator("#headerFooterSettingsDialog")).toBeHidden();

  const many = await inkOf(page, 1);
  expect(
    many.count,
    `a three-digit page number should ink more than a one-digit one ` +
      `(${one.count} -> ${many.count})`,
  ).toBeGreaterThan(one.count * 1.8);

  // And the FORMAT: 888 in upper Roman is not three digits, so the ink changes
  // again rather than staying where "start" left it.
  await openHeaderFooterSettings(page);
  await page.locator("#pageNumberStart").fill("8");
  await page.locator("#pageNumberFormat").selectOption("upperRoman");
  await page.locator("#headerFooterSettingsApply").click();
  await expect(page.locator("#headerFooterSettingsDialog")).toBeHidden();

  const roman = await inkOf(page, 1);
  // "VIII" is four glyphs against "8"'s one, so it inks more than the single digit
  // it replaced — which a format that never reached layout could not do.
  expect(
    roman.count,
    `VIII should ink more than 8 (${one.count} for "1", ${roman.count} for "VIII")`,
  ).toBeGreaterThan(one.count * 1.8);

  // Reopening the dialog reflects what the document now says, rather than what was
  // typed into it: the read side and the write side are the same payload.
  await openHeaderFooterSettings(page);
  await expect(page.locator("#pageNumberFormat")).toHaveValue("upperRoman");
  await expect(page.locator("#pageNumberStart")).toHaveValue("8");
  await expect(page.locator("#pageNumberContinue")).not.toBeChecked();

  expect(consoleErrors).toEqual([]);
});

test("the gutter moves the text column in from the binding edge", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await newBlankDocument(page);
  await page.keyboard.type("GUTTER");

  const before = await inkOf(page, 1);
  expect(before.minX, "the word has a leading edge to move").not.toBeNull();

  await openPageSetup(page);
  await expect(page.locator("#pageMarginGutter")).toHaveValue("0");
  await page.locator("#pageMarginGutter").fill("1");
  await page.locator("#pageSetupApply").click();
  await expect(page.locator("#pageSetupMenu")).toBeHidden();

  const after = await inkOf(page, 1);
  // A 1in binding margin is added to the inner edge, so the text starts further in.
  // The comparison is against the SAME raster width, so it needs no unit
  // conversion — only the direction and that the move is not noise.
  expect(
    after.minX,
    `the text's leading edge moved from ${before.minX} to ${after.minX} of ` +
      `${after.width}px when a 1in gutter was added`,
  ).toBeGreaterThan(before.minX + after.width * 0.05);

  expect(consoleErrors).toEqual([]);
});

test("Link to previous is offered, disabled, and says why — never a dead control", async ({
  page,
  consoleErrors,
}) => {
  await gotoSections(page);
  await clickIntoFirstPage(page);
  await openHeaderFooterSettings(page);

  const box = page.locator("#headerFooterLinkToPrevious");
  // Present and reflecting the document, because the engine already reports
  // inheritance per page (`runningBands` returns `headerLinked`/`footerLinked`).
  await expect(box).toBeVisible();
  await expect(box).toBeDisabled();

  // And the reason is ON SCREEN, not only in a tooltip: a hover has no equivalent
  // on a touch screen, and docs/126 is explicit that a command inside a surface
  // the user was offered must explain itself.
  const reason = page.locator("#headerFooterLinkReason");
  await expect(reason).toBeVisible();
  await expect(reason).not.toBeEmpty();

  expect(consoleErrors).toEqual([]);
});

test("every property this round shipped is reachable from at least two surfaces", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Surface 1: the Insert band's Header & footer group, where ONLYOFFICE files the
  // same controls.
  await openHeaderFooterSettings(page);
  await page.locator("#headerFooterSettingsCancel").click();
  await expect(page.locator("#headerFooterSettingsDialog")).toBeHidden();

  // Surface 2: the command palette.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  const row = page.locator('#cmdList .cmd-item[data-command-id="layout.headerFooterSettings"]');
  await expect(row).toBeVisible();
  await row.click();
  await expect(page.locator("#headerFooterSettingsDialog")).toBeVisible();
  await page.locator("#headerFooterSettingsClose").click();

  // Gutter and vertical alignment ride Page setup, which has four ribbon routes
  // plus the File page and the palette. Two of the ribbon routes are enough to
  // prove the fields are not behind a single door; both must land on a dialog that
  // actually carries them.
  for (const button of ["#layoutMarginsBtn", "#layoutSizeBtn"]) {
    await page.locator('[data-tab="layout"]').click();
    await page.locator(button).click();
    await expect(page.locator("#pageSetupMenu")).toBeVisible();
    await expect(page.locator("#pageMarginGutter")).toBeVisible();
    await expect(page.locator("#pageVerticalAlignment")).toBeVisible();
    await page.locator("#pageSetupCancel").click();
    await expect(page.locator("#pageSetupMenu")).toBeHidden();
  }

  expect(consoleErrors).toEqual([]);
});
