// The Save status line counts what the findings chip counts (`docs/109`
// FID-FW-01).
//
// FID-AT-05 stopped the chip counting Word's own bookkeeping — revision ids, a
// thumbnail an edit made stale, Word 2010's second copy of the styles — because
// none of it is something a reader would miss. The Save line kept its own
// count of every report entry, so an edited `sample.docx` saved as "Saved
// sample.docx with 2 compatibility findings" while no chip said there were any:
// two numbers for one report, and the alarming one was wrong. The guarantee
// asserted here is the reader's: after an ordinary edit and save, the status
// line and the chip agree, and neither counts the bookkeeping.
import { MOD, expect, test } from "./fixtures.mjs";

/** Opens the shipped `sample.docx` (what `/editor.html` opens with no fixture),
 *  waiting on the painted document rather than an empty status line — the same
 *  wait `compat-findings.spec.mjs` uses, for the reason it gives. */
async function openSample(page) {
  await page.goto("/editor.html");
  await page.waitForFunction(
    () =>
      document.querySelectorAll(".page-wrap").length > 0 && document.body.dataset.fontsReady === "true",
    null,
    { timeout: 45_000 },
  );
}

test("an edited save of sample.docx reports the findings the chip reports, not Word's bookkeeping", async ({
  page,
  consoleErrors,
}) => {
  await openSample(page);
  // An ordinary edit, so the save is a re-written package: the thumbnail is
  // now stale and the Word 2010 style copy is dropped — both bookkeeping.
  const sheet = page.locator(".page-wrap .page").first();
  await sheet.click({ position: { x: 120, y: 160 } });
  await page.keyboard.type("x");
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");

  // Every text the status line shows from here on, so the save's own sentence
  // is read even if a later, unrelated message (a font note) replaces it.
  await page.locator("#status").evaluate((el) => {
    window.__statusLog = [];
    new MutationObserver(() => window.__statusLog.push(el.textContent ?? "")).observe(el, {
      childList: true,
      characterData: true,
      subtree: true,
    });
  });
  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  await download;
  let saveLine = "";
  await expect
    .poll(async () => {
      const log = await page.evaluate(() => window.__statusLog);
      saveLine = log.find((text) => text.startsWith("Saved sample.docx")) ?? "";
      return saveLine;
    }, { message: "the save says what it saved" })
    .not.toBe("");

  // The chip is the count a reader is shown for the save's report; the status
  // line must say the same thing.
  const chip = page.locator("#compatibilityStatus");
  const chipCount = (await chip.isVisible())
    ? Number((await chip.textContent()).replace(/[^\d]/g, ""))
    : 0;
  if (chipCount === 0) {
    expect(saveLine, "no chip, so no findings to report on the save line").toBe("Saved sample.docx");
  } else {
    expect(saveLine).toContain(`with ${chipCount.toLocaleString("en-US")} compatibility finding`);
  }
  // And the report behind the save DID hold Word's bookkeeping, or this test
  // could not tell a fixed count from a report that was simply empty.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("compatibility");
  const row = page.locator('#cmdList .cmd-item[data-command-id="file.compatibilityReport"]');
  await expect(row, "the save's report has entries — the bookkeeping").toBeEnabled();
  await row.click();
  await expect(page.locator("#compatibilityFindingsDialog .findings-bookkeeping").first()).toBeVisible();
  expect(consoleErrors).toEqual([]);
});
