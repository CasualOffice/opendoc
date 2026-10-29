// Link to Previous, Remove and the page-number positions, in the browser
// (`docs/109` HF-192 and RM-23; `docs/168` batch 8).
//
// Every assertion that matters is about INK on a page, not host state:
// `docs/129` §6 is explicit that `headerLinked === false` proves nothing, and
// the earlier running-content specs were green while the wrong body was edited
// precisely because they read host state.
//
// The fixture (`?fixture=sections`) is four portrait pages whose section owns
// "PORTRAIT FOOTER", then two landscape pages whose section owns "LANDSCAPE
// FOOTER" — two sections, each with its own footer.
import { expect, pageSheet, runAppMenuCommand, runPaletteCommand, test } from "./fixtures.mjs";

const FIRST_PAGE = 0; // 0-based
const SECOND_SECTION_PAGE = 4; // page 5, the landscape section

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
}

function sheetOf(page, index) {
  return page.locator(`#pages .page-wrap[data-page-number="${index + 1}"]`);
}

async function showPage(page, index) {
  await pageSheet(page, index + 1);
  await sheetOf(page, index).evaluate((element) => element.scrollIntoView({ block: "center" }));
  await expect.poll(() => sheetOf(page, index).locator("canvas.page").count()).toBe(1);
}

/** The dark pixels of the bottom band of one page's raster, and where they sit:
 *  what the FOOTER paints there. */
async function footerInk(page, index) {
  await showPage(page, index);
  return page.evaluate((i) => {
    const canvas = document
      .querySelector(`#pages .page-wrap[data-page-number="${i + 1}"]`)
      .querySelector("canvas.page");
    const band = Math.max(1, Math.round(canvas.height * 0.2));
    const top = canvas.height - band;
    const data = canvas.getContext("2d").getImageData(0, top, canvas.width, band).data;
    let ink = 0;
    let moment = 0;
    for (let p = 0; p < data.length; p += 4) {
      if ((data[p] + data[p + 1] + data[p + 2]) / 3 < 160) {
        ink += 1;
        moment += (p / 4) % canvas.width;
      }
    }
    return { ink, signature: `${ink}:${moment}` };
  }, index);
}

async function openFooterOn(page, index) {
  await showPage(page, index);
  await runPaletteCommand(page, "insert.footer", "Edit footer");
  await expect(page.locator("#pages")).toHaveAttribute("data-running-edit", "footer");
  await expect(sheetOf(page, index).locator("#runningBandBar")).toBeVisible();
}

// Mutation (engine): point section 2 at the INHERITED body instead of the copy in
// `sections::set_linked_to_previous` — the typed text then lands in section 1's
// footer and the last assertion fails. Mutation (copy): copy as one empty
// paragraph — the band goes blank on unlink and the "looks the same" assertion fails.
test("Link to previous on the band bar: link, then unlink to an identical copy that edits only this section", async ({
  page,
  consoleErrors,
}) => {
  await gotoSections(page);
  const firstSection = (await footerInk(page, FIRST_PAGE)).signature;
  const own = (await footerInk(page, SECOND_SECTION_PAGE)).signature;
  expect(own, "the two sections start with different footers").not.toBe(firstSection);

  await openFooterOn(page, SECOND_SECTION_PAGE);
  const link = page.locator("#runningBandLink");
  await expect(link).toBeEnabled();
  await expect(link).not.toBeChecked();

  // Link: the page now shows the previous section's footer, and says so.
  await link.click();
  await expect(link).toBeChecked();
  await expect(page.locator("#status")).toContainText("now follows the previous section's");
  await expect(sheetOf(page, SECOND_SECTION_PAGE).locator(".running-band-label")).toContainText("Same as Previous");
  await expect.poll(async () => (await footerInk(page, SECOND_SECTION_PAGE)).signature).not.toBe(own);
  const linked = (await footerInk(page, SECOND_SECTION_PAGE)).signature;

  // Unlink: the section gets its own COPY, so the page looks exactly the same.
  await link.click();
  await expect(link).not.toBeChecked();
  await expect(page.locator("#status")).toContainText("is its own now");
  await expect(sheetOf(page, SECOND_SECTION_PAGE).locator(".running-band-label")).not.toContainText("Same as Previous");
  await expect.poll(async () => (await footerInk(page, SECOND_SECTION_PAGE)).signature).toBe(linked);

  // And editing it leaves the first section's footer alone.
  await page.keyboard.type("ZZ");
  await expect.poll(async () => (await footerInk(page, SECOND_SECTION_PAGE)).signature).not.toBe(linked);
  expect((await footerInk(page, FIRST_PAGE)).signature).toBe(firstSection);

  expect(consoleErrors).toEqual([]);
});

// Mutation: make `bandCommandReason` return "" on the first section — the box is
// then enabled on page 1 and the first assertion fails.
test("on the first section Link to previous is disabled and says why, on the bar and in the dialog", async ({
  page,
  consoleErrors,
}) => {
  await gotoSections(page);
  await openFooterOn(page, FIRST_PAGE);
  await expect(page.locator("#runningBandLink")).toBeDisabled();
  await expect(page.locator("#runningBandLinkWrap")).toHaveAttribute(
    "title",
    "The first section has no previous section to link to",
  );
  await page.keyboard.press("Escape");
  await expect(page.locator("#runningBandBar")).toBeHidden();

  // The dialog, from the body: one box per band, disabled here with the reason ON
  // SCREEN (a tooltip has no equivalent on a touch screen)…
  await showPage(page, FIRST_PAGE);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#headerFooterSettingsBtn").click();
  await expect(page.locator("#headerFooterSettingsDialog")).toBeVisible();
  await expect(page.locator("#headerFooterLinkHeader")).toBeDisabled();
  await expect(page.locator("#headerFooterLinkFooter")).toBeDisabled();
  await expect(page.locator("#headerFooterLinkReason")).toBeVisible();
  await page.locator("#headerFooterSettingsCancel").click();

  // …and live on the second section's pages.
  await showPage(page, SECOND_SECTION_PAGE);
  await page.locator("#headerFooterSettingsBtn").click();
  await expect(page.locator("#headerFooterLinkFooter")).toBeEnabled();
  await expect(page.locator("#headerFooterLinkFooter")).not.toBeChecked();
  await expect(page.locator("#headerFooterLinkReason")).toBeHidden();
  await page.locator("#headerFooterSettingsCancel").click();

  expect(consoleErrors).toEqual([]);
});

// Mutation: drop the `SetSectionRunningRef` from the no-band path of
// `sections::insert_page_number`, or align `Start` for every position — the first
// changes no ink, the second leaves the number where "bottom left" would put it.
test("a page-number position from the palette adds the number to every page of the section, keeping the footer", async ({
  page,
  consoleErrors,
}) => {
  await gotoSections(page);
  const before = await footerInk(page, FIRST_PAGE);
  const otherSection = (await footerInk(page, SECOND_SECTION_PAGE)).signature;

  await showPage(page, FIRST_PAGE);
  await runPaletteCommand(page, "layout.pageNumber.bottomRight", "Page number: bottom right");
  await expect(page.locator("#pages")).toHaveAttribute("data-running-edit", "footer");
  await expect(page.locator("#status")).toContainText("Page number added to the footer");

  // More ink than before: the number joined the footer, it did not replace it.
  await expect.poll(async () => (await footerInk(page, FIRST_PAGE)).ink).toBeGreaterThan(before.ink);
  // The second section owns its footer, so it did not change.
  expect((await footerInk(page, SECOND_SECTION_PAGE)).signature).toBe(otherSection);

  expect(consoleErrors).toEqual([]);
});

// Mutation: lower Remove as `SetSectionRunningRef { None }` always — section 2
// then inherits section 1's footer, its page keeps ink, and the `ink === 0`
// assertion fails.
test("Remove footer from the Insert menu takes it off this section only, and Undo brings it back", async ({
  page,
  consoleErrors,
}) => {
  await gotoSections(page);
  const firstSection = (await footerInk(page, FIRST_PAGE)).signature;
  const own = await footerInk(page, SECOND_SECTION_PAGE);
  expect(own.ink).toBeGreaterThan(0);

  await showPage(page, SECOND_SECTION_PAGE);
  await runAppMenuCommand(page, "insert", "layout.footer.remove");
  await expect(page.locator("#status")).toContainText("Footer removed from this section");
  await expect.poll(async () => (await footerInk(page, SECOND_SECTION_PAGE)).ink).toBe(0);
  expect((await footerInk(page, FIRST_PAGE)).signature).toBe(firstSection);

  // Undo from the palette: the menu that ran Remove may still hold focus, and a
  // chord sent there would never reach the editor.
  await runPaletteCommand(page, "edit.undo", "Undo");
  await expect.poll(async () => (await footerInk(page, SECOND_SECTION_PAGE)).signature).toBe(own.signature);

  expect(consoleErrors).toEqual([]);
});
