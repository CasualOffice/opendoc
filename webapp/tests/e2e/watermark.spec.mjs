// Watermarks, end to end (`109` OO-006).
//
// `casual-doc-layout/src/watermark.rs` has resolved a section's watermark to a
// per-page stamp and `compose_page` has painted it behind everything else since
// the layout half of OO-006 — and nothing in the editor could ask for one, so a
// document that needs DRAFT across every page could not get it and an imported
// one could not be removed.
//
// So the load-bearing assertion here is INK ON THE PAGE, not host state. A spec
// that asserted `doc.watermark()` came back with `kind: "text"` would pass
// against a dialog that writes the property and a renderer that draws nothing —
// which is the trap `line-numbers.spec.mjs` records, and the trap this row was:
// the property was already reachable from the model's side and no user could see
// a stamp.
//
// Two measurement pitfalls, both paid for once already in `line-numbers`:
//
//   * wait on `document.fonts.ready`, or the same unchanged page measures three
//     different numbers because a fallback face paints first and its glyphs are
//     a different width;
//   * compare DELTAS, never absolute thresholds. The bands below are not blank
//     to begin with.
//
// And one that is this feature's own: the default watermark grey (#c0c0c0) at
// the semitransparent 50% composites to about 223/255 over white paper, so
// `line-numbers`' "darker than 160" ink test would measure a stamped page as
// blank. Anything the renderer touched counts here instead.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

/** The vertical middle of page one, full width: where a stamp of either layout
 *  lands. Word centres the watermark on the page box, not on the text column. */
const CENTRE = { top: 0.44, height: 0.12 };

/** A band low on the page, below the demo document's text. An auto-sized
 *  DIAGONAL stamp runs corner to corner and crosses it (315° over A4 puts its
 *  ends at roughly 13% and 87% of the height); an auto-sized HORIZONTAL one is a
 *  single level line through the centre and cannot reach it. That is what makes
 *  this band the one place the layout CHOICE is visible rather than assumed. */
const LOWER = { top: 0.70, height: 0.16 };

/** Everything the diagonal stamp covers, for measurements about how much ink a
 *  particular string paints. */
const MIDDLE = { top: 0.32, height: 0.36 };

/** Pixels the renderer touched inside a band of page one, as a fraction of the
 *  sheet. */
async function ink(page, band) {
  await page.evaluate(() => document.fonts.ready);
  return page.evaluate(({ top, height }) => {
    const canvas = document
      .querySelector('#pages .page-wrap[data-page-number="1"]')
      .querySelector("canvas.page");
    const y = Math.round(canvas.height * top);
    const h = Math.max(1, Math.round(canvas.height * height));
    const { data } = canvas.getContext("2d").getImageData(0, y, canvas.width, h);
    let painted = 0;
    // "Not paper", not "dark": see the header. The paper is pure white, so any
    // pixel below 250 is something the renderer put there.
    for (let p = 0; p < data.length; p += 4) {
      if ((data[p] + data[p + 1] + data[p + 2]) / 3 < 250) painted += 1;
    }
    return painted;
  }, band);
}

async function openWatermark(page) {
  await page.locator('[data-tab="layout"]').click();
  await page.locator("#watermarkBtn").click();
  await expect(page.locator("#watermarkDialog")).toBeVisible();
}

/** Fills the dialog in and commits it. `size` is in points; omitted leaves Word's
 *  "Auto", which fits the stamp to the page. */
async function applyText(page, { text, layout = "diagonal", size } = {}) {
  await openWatermark(page);
  await page.locator("#watermarkKindText").check();
  if (text !== undefined) await page.locator("#watermarkText").fill(text);
  if (size !== undefined) {
    await page.locator("#watermarkSize").selectOption(String(size * 2));
  }
  await page.locator(`#watermarkLayoutSeg button[data-watermark-layout="${layout}"]`).click();
  await page.locator("#watermarkApply").click();
  await expect(page.locator("#watermarkDialog")).toBeHidden();
}

/** Which kind the dialog says the section has, read back from the engine by
 *  reopening it rather than from whatever the last click left on screen. */
async function reportedKind(page) {
  await openWatermark(page);
  return page
    .locator('#watermarkDialog input[name="watermarkKind"]:checked')
    .getAttribute("value");
}

test("a text watermark is painted across the page, and undo takes it away", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const blank = await ink(page, CENTRE);

  await openWatermark(page);
  expect(
    await page.locator('#watermarkDialog input[name="watermarkKind"]:checked').getAttribute("value"),
    "a section with no watermark opens on No watermark, as Word's dialog does",
  ).toBe("none");
  // And the text half is inert until it is chosen, rather than a live-looking
  // form that writes nothing. Asserted on a CONTROL inside the fieldset, not on
  // the fieldset: `toBeDisabled` answers for the elements a user can operate, and
  // a `<fieldset disabled>` reads as "enabled" to it while disabling everything
  // it contains — which is the guarantee that matters here anyway.
  await expect(page.locator("#watermarkText")).toBeDisabled();
  await page.keyboard.press("Escape");

  await applyText(page, { text: "CONFIDENTIAL" });
  await expect
    .poll(() => ink(page, CENTRE), { message: "the engine stamps the page once asked" })
    .toBeGreaterThan(blank);

  // The dialog reads the section back rather than remembering what it wrote.
  expect(await reportedKind(page)).toBe("text");
  await expect(page.locator("#watermarkText")).toHaveValue("CONFIDENTIAL");
  await page.keyboard.press("Escape");

  await page.keyboard.press(`${MOD}+z`);
  await expect
    .poll(() => ink(page, CENTRE), { message: "undo takes the stamp off the page" })
    .toBe(blank);
  // A separate claim from the ink: undo changes the document without going
  // through this dialog, so only a dialog that reflects the engine as it OPENS
  // can answer here. Asserting it after a write would prove nothing.
  expect(await reportedKind(page), "undone, the dialog reads off again").toBe("none");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("No watermark removes the one that is there", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const blank = await ink(page, CENTRE);
  await applyText(page, { text: "DRAFT" });
  await expect.poll(() => ink(page, CENTRE)).toBeGreaterThan(blank);

  await openWatermark(page);
  await page.locator("#watermarkKindNone").check();
  await expect(
    page.locator("#watermarkText"),
    "choosing No watermark greys the text half rather than hiding it",
  ).toBeDisabled();
  await page.locator("#watermarkApply").click();
  await expect(page.locator("#watermarkDialog")).toBeHidden();

  await expect
    .poll(() => ink(page, CENTRE), { message: "No watermark clears the page" })
    .toBe(blank);
  expect(await reportedKind(page)).toBe("none");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("Diagonal and Horizontal are different stamps, not the same one relabelled", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const blankCentre = await ink(page, CENTRE);
  const blankLower = await ink(page, LOWER);

  await applyText(page, { text: "CONFIDENTIAL", layout: "diagonal" });
  await expect
    .poll(() => ink(page, LOWER), {
      message: "a diagonal stamp runs corner to corner, so it reaches low on the page",
    })
    .toBeGreaterThan(blankLower);
  const diagonalCentre = await ink(page, CENTRE);
  expect(diagonalCentre).toBeGreaterThan(blankCentre);

  await applyText(page, { text: "CONFIDENTIAL", layout: "horizontal" });
  // Still stamped — "less ink low down" must not be satisfiable by removing the
  // watermark, which is what an assertion on LOWER alone would accept.
  await expect
    .poll(() => ink(page, CENTRE), { message: "the horizontal stamp is still on the page" })
    .toBeGreaterThan(blankCentre);
  expect(
    await ink(page, LOWER),
    "a level stamp is one line through the centre and cannot reach the lower band",
  ).toBe(blankLower);

  // And the control says which way it is set, read back from the engine.
  await openWatermark(page);
  await expect(
    page.locator('#watermarkLayoutSeg button[data-watermark-layout="horizontal"]'),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(
    page.locator('#watermarkLayoutSeg button[data-watermark-layout="diagonal"]'),
  ).toHaveAttribute("aria-pressed", "false");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("the words in the field are the words on the page", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // At a FIXED size the amount of ink is a function of the string, which is what
  // makes this a measurement of the field's value reaching the engine rather
  // than of the auto-fit: auto-fit scales any string to the same span, so a
  // dialog that ignored the field and sent a constant would measure identical.
  await applyText(page, { text: "W", size: 72 });
  const one = await ink(page, MIDDLE);

  await applyText(page, { text: "WWWWWWWWWW", size: 72 });
  const ten = await ink(page, MIDDLE);
  expect(ten, "ten letters paint more than one at the same size").toBeGreaterThan(one * 3);

  // Back again: the third measurement is what rules out "the ink changed because
  // something else changed". A constant-sending dialog cannot produce all three.
  await applyText(page, { text: "W", size: 72 });
  await expect.poll(() => ink(page, MIDDLE)).toBe(one);
  await expect(page.locator("#watermarkSize")).toHaveValue("144");

  expect(consoleErrors).toEqual([]);
});

test("an empty stamp is refused out loud, not silently accepted", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const blank = await ink(page, CENTRE);
  await openWatermark(page);
  await page.locator("#watermarkKindText").check();
  await page.locator("#watermarkText").fill("   ");
  await page.locator("#watermarkApply").click();

  // The engine refuses an empty stamp and leaves the section exactly as it was,
  // so an Apply that closed the dialog would be a button that appears to work
  // and does nothing. It stays open on the offending field instead. Three spaces
  // rather than an empty field on purpose: `required` accepts whitespace, so this
  // is the case a validation-only refusal would have let through in silence.
  await expect(page.locator("#watermarkDialog")).toBeVisible();
  expect(
    await page.locator("#watermarkText").evaluate((el) => el.checkValidity()),
    "the field itself has to say it is the problem",
  ).toBe(false);
  expect(await ink(page, CENTRE)).toBe(blank);

  await page.keyboard.press("Escape");
  await expect(page.locator("#watermarkDialog")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("the picture half says why it is unavailable instead of doing nothing", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openWatermark(page);

  // docs/63's rule: a command with nothing behind it ships disabled CARRYING THE
  // REASON. A picture watermark needs a media id the document already holds and
  // the host cannot register one, so the choice is present, refused, and explains
  // itself — the alternative this repo keeps shipping by accident is a live
  // control that silently fails.
  const picture = page.locator("#watermarkKindPicture");
  await expect(picture).toBeDisabled();
  await expect(picture.locator("xpath=..")).toHaveAttribute(
    "title",
    /image already registered in the document/i,
  );

  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("the command is reachable from the palette as well as the ribbon", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // `109` UX-005 and the command-surface rule: a capability reachable from
  // exactly one surface IS the defect. Word puts Watermark on a Design tab this
  // product does not have, so the ribbon face is in Layout ▸ Page Setup and the
  // palette row has to open the same dialog rather than being a row that does
  // nothing.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("watermark");
  await page.locator(".cmd-item", { hasText: "Watermark" }).first().click();
  await expect(page.locator("#watermarkDialog")).toBeVisible();
});
