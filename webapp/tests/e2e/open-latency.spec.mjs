// Opening the editor must not wait on requests that could have run together
// (docs/109 HF-421). Measured on the deployed site, two independent downloads
// were chained behind others:
//
//   * the startup document was requested only after the engine had downloaded
//     and compiled (`await init()` then `fetch`), one extra round trip on the
//     critical path of every open;
//   * the first-party add-in manifests were requested one at a time, each after
//     the previous had landed (`for … await admit(entry)`).
//
// Both are asserted as ORDERING guarantees with a barrier, not as times: a
// response is held until the request that must overlap it has been made. With
// the old chain that request is never made while the response is held, so the
// barrier runs out and the assertion fails — on any machine, at any speed.
import { test, expect } from "./fixtures.mjs";

const HOLD_MS = 20_000;

/** A gate: `wait()` resolves true once `open()` is called, or false after `ms`. */
function barrier(ms) {
  let open;
  const opened = new Promise((resolve) => (open = resolve));
  return { open: () => open(true), wait: () => Promise.race([opened, new Promise((r) => setTimeout(() => r(false), ms))]) };
}

test("the startup document downloads while the engine is still downloading", async ({ page }) => {
  test.setTimeout(90_000);
  const documentAsked = barrier(HOLD_MS);
  let askedWhileEngineHeld = null;
  page.on("request", (request) => {
    if (new URL(request.url()).pathname.endsWith("/sample.docx")) documentAsked.open();
  });
  // The engine binary (and its preload) is held until the document is asked for.
  await page.route(/casual_doc_wasm_bg\.wasm/, async (route) => {
    const asked = await documentAsked.wait();
    if (askedWhileEngineHeld === null) askedWhileEngineHeld = asked;
    await route.continue();
  });
  await page.goto("/editor.html");
  await page.waitForFunction(() => document.body.classList.contains("doc-loaded"), null, { timeout: 60_000 });
  expect(askedWhileEngineHeld, "sample.docx was not requested until the engine had loaded").toBe(true);
});

test("the add-in manifests are requested together, not one after another", async ({ page }) => {
  test.setTimeout(90_000);
  const catalogue = await (await page.request.get("/plugins/catalogue.json")).json();
  const expected = catalogue.plugins.length;
  expect(expected, "the first-party catalogue lists several add-ins").toBeGreaterThan(1);
  const asked = new Set();
  const everyoneAsked = barrier(HOLD_MS);
  let allAskedWhileHeld = null;
  let askedAtFirstAnswer = null;
  // No manifest answers until every manifest has been asked for.
  await page.route(/\/plugins\/[^/]+\/manifest\.json/, async (route) => {
    asked.add(route.request().url());
    if (asked.size === expected) everyoneAsked.open();
    const all = await everyoneAsked.wait();
    if (askedAtFirstAnswer === null) askedAtFirstAnswer = asked.size;
    if (allAskedWhileHeld === null || !all) allAskedWhileHeld = all;
    await route.continue();
  });
  await page.goto("/editor.html");
  await page.waitForFunction(() => document.body.classList.contains("doc-loaded"), null, { timeout: 60_000 });
  await expect.poll(() => asked.size, { timeout: HOLD_MS * expected + 10_000 }).toBe(expected);
  expect(allAskedWhileHeld, `manifests were requested one at a time (${askedAtFirstAnswer} of ${expected} asked for before the first answer)`).toBe(true);
});
