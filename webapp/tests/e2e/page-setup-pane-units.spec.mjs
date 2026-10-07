// A distance and its unit label must come from the SAME unit, on every route
// that paints one.
//
// The File page's Page setup pane showed `21.59` and `27.94` — the centimetre
// readings of a Letter page — beside fields labelled `in`, while the caption
// under the preview said `21.59 × 27.94 cm` and the preference read
// Centimetres. It happened whenever the pane was the FIRST thing to show the
// page geometry in a session: the dialog route (`toggle`) applied the unit to
// each field's suffix, the pane route (`reflect`) painted the values in the
// reader's unit and never touched the suffix, so it kept the markup's `in`. A
// reader who "corrected" a field labelled inches would have typed inches into a
// centimetre field — an 8.5 that becomes an 8.5 cm page. The Line numbers
// popover's "From text" field had the same gap.
//
// The locale is pinned to a centimetre REGION rather than relying on the host:
// `defaultMeasurementUnit` answers inches for the US and Canada only, and on an
// inch machine the markup's `in` happens to be right — which is exactly the
// coincidence that hid this. The precondition is asserted, so a change to that
// default fails here by name rather than turning the guard into a no-op.
import { test, expect, gotoEditor, openFilePage, measurementSuffix } from "./fixtures.mjs";

test.use({ locale: "en-GB" });

/** Every distance suffix inside `root`, de-duplicated. */
async function suffixesIn(page, selector) {
  return page
    .locator(`${selector} [data-measure-suffix]`)
    .evaluateAll((nodes) => [...new Set(nodes.map((node) => node.textContent))]);
}

test("the File page's Page setup pane labels every field in the unit its value is written in", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  // Precondition: centimetres are in force, and NOTHING has opened Page setup —
  // the dialog route would have applied the unit and hidden the defect.
  expect(await measurementSuffix(page), "this guard needs a centimetre preference").toBe("cm");

  await openFilePage(page);
  const row = page.locator('#filePageBody [data-file-pane="pageSetup"]');
  await row.click();
  await expect(page.locator("#filePageDetail #pageSetupMenu")).toBeVisible();

  expect(await suffixesIn(page, "#pageSetupMenu"), "a field is labelled in another unit").toEqual(["cm"]);

  // And the value really is in that unit: the caption prints the same width with
  // the unit's own suffix, from the same twips, through a different path.
  const caption = await page.locator("#pagePreviewLabel").textContent();
  const [, captionWidth, captionUnit] = /^([\d.,]+) × [\d.,]+ (\S+)$/.exec(caption.trim()) ?? [];
  expect(captionUnit).toBe("cm");
  // Numerically: a field trims trailing zeros (`21`), the caption does not (`21.00`).
  const fieldWidth = Number(await page.locator("#pageWidth").inputValue());
  expect(fieldWidth).toBeCloseTo(Number(captionWidth.replace(",", ".")), 2);
  expect(consoleErrors).toEqual([]);
});

test("the Line numbers popover labels its distance in the unit in force", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  expect(await measurementSuffix(page)).toBe("cm");
  // Straight to the popover, before Page setup has ever been opened.
  await page.locator('[data-tab="layout"]').click();
  await page.locator("#lineNumbersBtn").click();
  await expect(page.locator("#lineNumbersMenu")).toBeVisible();
  await expect(page.locator("#lineNumberDistance ~ [data-measure-suffix]")).toHaveText("cm");
  expect(consoleErrors).toEqual([]);
});
