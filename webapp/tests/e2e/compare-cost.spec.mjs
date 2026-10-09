// What a comparison costs the main thread, measured — and guarded as a SHAPE
// rather than as a millisecond budget.
//
// SKILL §8: anything O(document) must not run on the main thread, must show real
// progress, and must be cancellable; and "guard complexity, not milliseconds — a
// timing threshold is flaky and cannot tell a slow constant from a quadratic".
// `comparableBytes` exports the live document in one synchronous call into wasm
// before a comparison can start, so this file asks the two questions that
// matter: is it linear, and does the reader get feedback and a way out before it
// runs.
//
// The numbers it was written against, in Chromium on 2026-10-04, over plain-text
// documents of n / 2n / 4n paragraphs on the regenerating export ladder (the one
// an edited document takes, because `exact_if_unchanged` refuses an edit):
//
//     5,000 paragraphs    358,896 bytes    12 ms
//    10,000 paragraphs    718,896 bytes    23 ms
//    20,000 paragraphs  1,448,896 bytes    44 ms
//
// 1.9× per doubling. The constant is small and the slope is the point: the same
// line at the viewer's own admission ceiling is seconds of work nothing can
// interrupt.
//
// PLAIN TEXT, not DOCX, and deliberately: a fixture whose exact content is one
// loop in this file makes "n paragraphs" a fact rather than a claim about a
// binary nobody here can read, and both sides go through the same format registry
// a DOCX would.
import { expect, test } from "./fixtures.mjs";

/** `lines` paragraphs of realistic prose. */
function textFile(name, lines) {
  const body = Array.from(
    { length: lines },
    (_, index) =>
      `Paragraph ${index} with enough words in it to be a realistic line of prose.`,
  ).join("\n");
  return { name, mimeType: "text/plain", buffer: Buffer.from(body, "utf8") };
}

/** Runs one comparison of an EDITED document of `lines` paragraphs and returns
 *  the export's own duration, from the `opendoc.compare.export` measure the
 *  module publishes.
 *
 *  The document is edited first so `exact_if_unchanged` refuses and the export
 *  falls to the regenerating ladder — the cost a comparison after real work
 *  really pays. Measuring the passthrough instead would be measuring a
 *  `memcpy`. */
async function exportCostFor(page, lines) {
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("#file")).toBeEnabled();
  await page.locator("#file").setInputFiles(textFile(`n${lines}.txt`, lines));
  await expect(page.locator("#docTitle")).toHaveValue(`n${lines}.txt`, { timeout: 180_000 });
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 180_000 });

  await page.locator(".page-wrap").first().click({ position: { x: 120, y: 120 } });
  await page.keyboard.insertText("edited ");

  await page.evaluate(() => performance.clearMeasures());
  await page.locator("#railCompare").click();
  await expect(page.locator("#comparePanel")).toBeVisible();
  // The other side differs by three paragraphs, so the comparison has a real
  // answer to produce and the redline below is not an empty one.
  await page.locator("#compareFile").setInputFiles(textFile("other.txt", lines - 3));
  await expect(page.locator("#compareBody [data-compare-view]")).toBeVisible({
    timeout: 300_000,
  });

  return page.evaluate(() => {
    const exports = performance.getEntriesByName("opendoc.compare.export");
    const progress = performance.getEntriesByName("opendoc.compare.progress");
    return {
      exportMs: exports.length > 0 ? exports[exports.length - 1].duration : -1,
      exportAt: exports.length > 0 ? exports[exports.length - 1].startTime : -1,
      progressAt: progress.length > 0 ? progress[progress.length - 1].startTime : -1,
    };
  });
}

test("the pre-comparison export is linear in document size, not quadratic", async ({ page }) => {
  // Three sizes rather than two: with two, one slow run makes a ratio and a
  // ratio made of one measurement is not a shape. With n, 2n and 4n a quadratic
  // has to hide from both doublings.
  test.setTimeout(600_000);
  const small = await exportCostFor(page, 5_000);
  const middle = await exportCostFor(page, 10_000);
  const large = await exportCostFor(page, 20_000);

  // The instrument really fired. Without this the ratios below could be computed
  // from two `-1`s and pass.
  expect(small.exportMs, "the export measure is missing").toBeGreaterThan(0);
  expect(middle.exportMs).toBeGreaterThan(0);
  expect(large.exportMs).toBeGreaterThan(0);
  // And it is big enough for a ratio to mean anything. Measured at 12 / 23 / 44
  // ms, so 4 ms is a floor a healthy run clears by 3×; a run that does not clear
  // it is timer noise and a ratio over it would be noise too.
  expect(
    small.exportMs,
    `the export took ${small.exportMs.toFixed(1)} ms — too small for a ratio to mean anything`,
  ).toBeGreaterThan(4);

  // THE SHAPE. Doubling the document must roughly double the work. Linear is
  // 2.0, quadratic is 4.0, and the bound sits between them with room for a noisy
  // runner: a quadratic export cannot pass this and a linear one cannot fail it
  // for being slow.
  const firstStep = middle.exportMs / small.exportMs;
  const secondStep = large.exportMs / middle.exportMs;
  const message =
    `export: ${small.exportMs.toFixed(1)} ms at 5,000 paragraphs, ` +
    `${middle.exportMs.toFixed(1)} ms at 10,000 (${firstStep.toFixed(2)}×), ` +
    `${large.exportMs.toFixed(1)} ms at 20,000 (${secondStep.toFixed(2)}×)`;
  expect(firstStep, `${message} — the first doubling cost more than doubly`).toBeLessThan(3);
  expect(secondStep, `${message} — the second doubling cost more than doubly`).toBeLessThan(3);
  console.log(`[compare-cost] ${message}`);
});

test("the reader is given progress and a Cancel BEFORE the export blocks", async ({ page }) => {
  // SKILL §8's other two requirements. This surface exported first and rendered
  // its progress bar afterwards, so the one document-sized call on the path ran
  // with the chooser still on screen and no way out — the frozen tab the rule is
  // about, and invisible to every test that only checked the final result.
  //
  // Asserted through the marks rather than by racing the DOM: catching a
  // mid-export repaint from a spec is a timing fight, and "the progress state was
  // painted before the export started" is exactly what the two timestamps say.
  test.setTimeout(300_000);
  const measured = await exportCostFor(page, 10_000);
  expect(measured.progressAt, "no progress mark, so nothing was painted").toBeGreaterThan(0);
  expect(
    measured.progressAt,
    `progress was painted at ${measured.progressAt.toFixed(1)} ms and the export ` +
      `started at ${measured.exportAt.toFixed(1)} ms — the reader saw nothing until after the block`,
  ).toBeLessThan(measured.exportAt);

  // And the Cancel the progress state carries really reaches the surface — a
  // cancellable job whose Cancel is never rendered is not cancellable.
  //
  // RECORDED, NOT RACED. The first attempt at this clicked the button, and it
  // flaked for a reason worth writing down rather than retrying away: on these
  // fixtures the whole comparison finishes in a few hundred milliseconds, so the
  // button is gone between `toBeVisible` passing and the click landing. Waiting
  // for a slower document would make the guard a clock, which SKILL §6 names as
  // the failure that retries do not help. A `MutationObserver` installed BEFORE
  // the gesture records every state the panel passed through, so the assertion is
  // on what was really on screen and cannot be outrun.
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("#file")).toBeEnabled();
  await page.locator("#file").setInputFiles(textFile("states.txt", 10_000));
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 180_000 });
  await page.locator("#railCompare").click();
  await expect(page.locator("#comparePanel")).toBeVisible();
  await page.evaluate(() => {
    window.__states = [];
    const body = document.getElementById("compareBody");
    const record = () =>
      window.__states.push({
        cancel: Boolean(body.querySelector('[data-compare-action="cancel"]')),
        progress: Boolean(body.querySelector("[data-compare-progress]")),
        total: Boolean(body.querySelector("[data-compare-view]")),
      });
    record();
    window.__stateObserver = new MutationObserver(record);
    window.__stateObserver.observe(body, { childList: true, subtree: true });
  });
  await page.locator("#compareFile").setInputFiles(textFile("states-other.txt", 9_997));
  await expect(page.locator("#compareBody [data-compare-view]")).toBeVisible({
    timeout: 300_000,
  });
  const states = await page.evaluate(() => {
    window.__stateObserver.disconnect();
    return window.__states;
  });

  const firstResult = states.findIndex((state) => state.total);
  expect(firstResult, "no result state was recorded, so nothing below is tested").toBeGreaterThan(
    -1,
  );
  const withCancel = states.findIndex((state) => state.cancel && state.progress);
  expect(
    withCancel,
    `the panel went from ${JSON.stringify(states[0])} straight to a result without ever ` +
      "showing progress and a Cancel",
  ).toBeGreaterThan(-1);
  expect(
    withCancel,
    "progress and Cancel appeared only after the result, which is no use to anybody",
  ).toBeLessThan(firstResult);
});
