// Line numbers, end to end (`109` OO-022).
//
// The engine has laid out and painted `w:lnNumType` since P1F-36
// (`casual-doc-layout/src/line_number.rs`) and nothing could ask for it, so a
// court filing that needs 28 numbered lines per page opened with the numbers
// silently gone. The gap was the whole route from the ribbon to the paint.
//
// So the load-bearing assertion here is INK IN THE MARGIN, not host state. A
// spec that asserted `lineNumbering()` came back with `countBy: 5` would pass
// against a UI that writes the property and a renderer that draws nothing —
// which is exactly what this row was, since the property was already reachable
// from the model's side and no user could see a number. `section-running-content`
// records the same trap ("the previous specs were green while the wrong body was
// being edited precisely because they only asserted host state").
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

/** Ink in the leading margin of page one — the strip a line number is painted
 *  into. `place_line_numbers` right-aligns each number `w:distance` to the left
 *  of the text column, which on this document puts them between 5% and 10% of
 *  the sheet width.
 *
 *  Waiting on `document.fonts.ready` is what makes this a measurement rather
 *  than a coin toss: without it the same unchanged page measured 234, then 20,
 *  then 25, because a fallback face paints first and its glyphs are a different
 *  width. Every number below was read off this helper, and they are exact — the
 *  document is fixed, so the counts are reproducible rather than approximate. */
async function marginInk(page) {
  await page.evaluate(() => document.fonts.ready);
  return page.evaluate(() => {
    const canvas = document
      .querySelector('#pages .page-wrap[data-page-number="1"]')
      .querySelector("canvas.page");
    const width = Math.max(1, Math.round(canvas.width * 0.1));
    const top = Math.round(canvas.height * 0.06);
    const height = Math.round(canvas.height * 0.88);
    const { data } = canvas.getContext("2d").getImageData(0, top, width, height);
    let ink = 0;
    for (let p = 0; p < data.length; p += 4) {
      if ((data[p] + data[p + 1] + data[p + 2]) / 3 < 160) ink += 1;
    }
    return ink;
  });
}

/** Opens the Line Numbers popover, whether or not it is already up — the
 *  trigger TOGGLES, so an unconditional click closes a menu left open by the
 *  previous step and the next action then runs against a hidden control. */
async function openLineNumbers(page) {
  await page.locator('[data-tab="layout"]').click();
  if (!(await page.locator("#lineNumbersMenu").isVisible())) {
    await page.locator("#lineNumbersBtn").click();
  }
  await expect(page.locator("#lineNumbersMenu")).toBeVisible();
}

/** Which preset the menu says is active, read back from the engine. */
async function activePreset(page) {
  return page
    .locator('#lineNumbersMenu [data-linenumber][aria-checked="true"]')
    .getAttribute("data-linenumber");
}

async function pickPreset(page, mode) {
  await openLineNumbers(page);
  await page.locator(`#lineNumbersMenu [data-linenumber="${mode}"]`).click();
}

test("asking for line numbers puts numbers in the margin, and undo takes them away", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // 234 with no numbering: the strip is not blank to begin with, which is why
  // every assertion here is a delta and none is a threshold.
  const unnumbered = await marginInk(page);

  await openLineNumbers(page);
  expect(await activePreset(page), "numbering starts off").toBe("none");
  await page.locator('#lineNumbersMenu [data-linenumber="newPage"]').click();

  await expect
    .poll(() => marginInk(page), {
      message: "the engine paints a number per line once a section asks for it",
    })
    .toBeGreaterThan(unnumbered);

  // And the control now says which way it is set — reopened, so the answer comes
  // from the engine rather than from whatever the click left on screen.
  await openLineNumbers(page);
  expect(await activePreset(page)).toBe("newPage");
  await page.keyboard.press("Escape");

  await page.keyboard.press("Control+z");
  await expect
    .poll(() => marginInk(page), { message: "undo clears the numbers" })
    .toBe(unnumbered);

  // And the menu agrees, which is a different claim from the ink: undo changes
  // the document without going through this module, so the only thing that can
  // answer here is the popover reflecting the engine as it opens. Asserting it
  // after a WRITE would prove nothing — the write path reflects on its own way
  // out, so a popover that never reflected at all measured green until this.
  await openLineNumbers(page);
  expect(await activePreset(page), "undone, the control reads off again").toBe("none");
  expect(consoleErrors).toEqual([]);
});

test("None turns numbering off, and leaves no rule behind", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const unnumbered = await marginInk(page);
  await pickPreset(page, "newPage");
  await expect.poll(() => marginInk(page)).toBeGreaterThan(unnumbered);

  await pickPreset(page, "none");
  await expect
    .poll(() => marginInk(page), { message: "None clears the margin" })
    .toBe(unnumbered);

  // Off means the EMPTY rule, not a rule that happens to number nothing — the
  // interpretation import, layout and export share
  // (`casual-doc-layout/src/line_number.rs`), pinned on the engine side by
  // `turning_line_numbering_off_installs_the_empty_rule`. Here it shows up as
  // the count step being back at its default rather than at whatever the
  // clearing path wrote: a None that sent `countBy: 0` would be clamped back to
  // 1 by layout and number every line again, which is off in the menu and fully
  // numbered on the page.
  await openLineNumbers(page);
  expect(await activePreset(page)).toBe("none");
  expect(await page.locator("#lineNumberCountBy").inputValue()).toBe("1");
});

test("Count by numbers every Nth line rather than every line", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const unnumbered = await marginInk(page);
  await pickPreset(page, "newPage");
  await expect.poll(() => marginInk(page)).toBeGreaterThan(unnumbered);
  const everyLine = await marginInk(page);

  // Two, not five. Page one of the demo document is FOUR lines, so `countBy: 5`
  // correctly numbers nothing at all and measures identical to an unnumbered
  // page — a true result that would read here as a broken feature. Measured:
  // 234 unnumbered, 402 every line, 261 every second, 264 every third, 234
  // every fifth.
  await openLineNumbers(page);
  await page.locator("#lineNumberCountBy").fill("2");
  await page.locator("#lineNumberCountBy").press("Enter");
  await page.keyboard.press("Escape");

  await expect
    .poll(() => marginInk(page), {
      message: "one number in two is less ink than one per line",
    })
    .toBeLessThan(everyLine);
  // Still numbering, though — "less ink" must not be satisfied by turning it
  // off, which is what an assertion against `everyLine` alone would accept.
  expect(await marginInk(page)).toBeGreaterThan(unnumbered);

  await openLineNumbers(page);
  expect(await page.locator("#lineNumberCountBy").inputValue()).toBe("2");
  expect(await activePreset(page), "changing the step does not turn numbering off").toBe("newPage");
});

test("skipping a paragraph takes it out of the count, and says so when reopened", async ({
  page,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const unnumbered = await marginInk(page);
  await pickPreset(page, "newPage");
  await expect.poll(() => marginInk(page)).toBeGreaterThan(unnumbered);
  const counted = await marginInk(page);

  await openLineNumbers(page);
  await expect(page.locator("#lineNumberSuppress")).not.toBeChecked();
  await page.locator("#lineNumberSuppress").check();
  await page.keyboard.press("Escape");

  // The caret's paragraph loses its numbers, and the page keeps the rest: 402
  // numbered, 293 with this paragraph skipped, 234 with no numbering at all.
  // That it is the RIGHT paragraph, and that a style can carry the flag as a
  // pleading template does, are asserted where they can be asserted exactly —
  // `suppressing_a_paragraph_is_visible_on_the_read_side` and
  // `suppression_inherited_from_a_style_is_reported_too` in the wasm crate.
  await expect.poll(() => marginInk(page)).toBeLessThan(counted);
  expect(await marginInk(page), "the rest of the page is still numbered").toBeGreaterThan(
    unnumbered,
  );

  await openLineNumbers(page);
  await expect(
    page.locator("#lineNumberSuppress"),
    "reopened, the checkbox reflects the paragraph's own state",
  ).toBeChecked();
  await page.locator("#lineNumberSuppress").uncheck();
  await page.keyboard.press("Escape");
  await expect
    .poll(() => marginInk(page), { message: "putting it back restores every number" })
    .toBe(counted);
});

test("the control is reachable from the palette as well as the ribbon", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // `109` UX-005 and the command-surface rule: a capability reachable from
  // exactly one surface IS the defect, so the palette row has to open the same
  // popover rather than being a row that does nothing. It is declared
  // `ownsClick`, which is the one thing that could make it a dead row.
  await page.keyboard.press("Control+Shift+P");
  await page.locator("#cmdInput").fill("line numbers");
  await page.locator(".cmd-item", { hasText: "Line numbers" }).first().click();
  await expect(page.locator("#lineNumbersMenu")).toBeVisible();
});
