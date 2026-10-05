// CJK in a document asks for a covering face from somewhere it is actually
// served from, prefers our own origin whenever we have it, and says so when it
// cannot get one at all.
//
// `109` HF-176: more than half the text of two corpus documents rendered as
// notdef boxes. Two separate things had to be true for that, and this file and
// the Rust guards split them:
//
//   * **Does the engine draw CJK once a face is registered?** Proved in
//     `casual-doc-layout/tests/cjk_coverage.rs` (the resolved face's own `cmap`
//     maps each scalar) and `casual-doc-render/tests/cjk_paint.rs` (the raster
//     differs from the notdef box's). Those are deterministic and offline.
//   * **Does the browser ask for that face, from us, when a document needs
//     it?** That is this file. A unit test over `fontKeyForCodePoint` would
//     still pass if the fetch/register path never ran — the same reasoning
//     `emoji-rendering.spec.mjs` records for the emoji bucket.
//
// The assertions here are deliberately about the REQUEST and the REFUSAL, not
// about painted CJK pixels. Loading a real CJK face in a browser test would mean
// a ~16.4 MB download, and the mirrored faces are not committed, so a spec that
// waited for glyphs would either need the network in CI or would silently assert
// nothing when the fetch failed. The painted-pixel guarantee is proved by the
// Rust guards instead, where it can be deterministic.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

/** Our own origin's script-font directory — where every COMMITTED document font
 *  comes from, and every mirrored one too on a deployment that has declared them
 *  provisioned. Matching the path rather than a full URL keeps this correct for
 *  whatever host:port the test server is on. */
const OUR_ORIGIN_FONTS = /\/assets\/fonts\/script\//;

/** Route pattern for the CJK faces alone.
 *
 * Deliberately NOT the whole font directory. The named Latin faces are
 * committed and fetched eagerly, and `gotoEditor` waits for
 * `document.body.dataset.fontsReady`, so aborting the directory wholesale hangs
 * the editor before the test has begun — which is how the first version of this
 * spec failed, with a timeout in the fixture rather than an assertion.
 */
const CJK_ROUTE = "**/assets/fonts/script/NotoSansCJK*";

/** The third-party CDN every document font used to come from, and which the
 *  four non-committed faces still fall back to until a deployment provisions
 *  them. No face this repository actually contains may reach it. */
const THIRD_PARTY = /cdn\.jsdelivr\.net/;

/** A CJK region face, by filename. Matching the file rather than a bucket key
 *  survives a change of region without going quietly green — the mistake
 *  `emoji-rendering.spec.mjs` records making twice with its own matcher. */
const CJK_FACE = /NotoSansCJK(jp|kr|sc)-Regular\.otf$/;

test("typing CJK asks for a CJK face, and never speculatively against our origin", async ({
  page,
}) => {
  const requests = [];
  page.on("request", (request) => requests.push(request.url()));

  // Fail the CJK requests, so the test neither downloads 16.4 MB nor depends on
  // the network, while the committed Latin faces still load and the editor still
  // reaches its ready state. What is under test is which URL is asked for.
  await page.route(CJK_ROUTE, (route) => route.abort());
  await page.route("**/cdn.jsdelivr.net/**", (route) => route.abort());

  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // The demo fixture carries no CJK, so nothing CJK should have been asked for:
  // the bucket is coverage-driven and a Latin document must not pay for it.
  expect(requests.filter((url) => CJK_FACE.test(url))).toEqual([]);

  await page.keyboard.insertText("中文字");
  await expect(page.locator("#a11yDocument")).toContainText("中文字");

  await expect
    .poll(() => requests.filter((url) => CJK_FACE.test(url)).length, {
      timeout: 15_000,
    })
    .toBeGreaterThan(0);

  // This checkout has not declared the CJK faces provisioned — the default, and
  // what CI and a fresh clone are in — so the FIRST request goes to the pinned
  // mirror. Asking our own origin first here would be a guaranteed 404: the file
  // is not in the repository. A 404-then-fallback is what the first version of
  // this work did, and it cost a console error per face on every unprovisioned
  // deployment.
  const cjk = requests.filter((url) => CJK_FACE.test(url));
  expect(cjk[0]).toMatch(THIRD_PARTY);
  expect(cjk[0]).toMatch(/@[0-9a-f]{40}\//); // commit-pinned, hash-verified
});

test("a deployment that provisioned the CJK faces asks only itself", async ({
  page,
}) => {
  const requests = [];
  page.on("request", (request) => requests.push(request.url()));

  // What a deployment that has run `tools/provision-script-fonts.mjs` declares.
  // `addInitScript` runs before the editor's module graph, which is the ordering
  // the manifest documents for this global.
  await page.addInitScript(() => {
    window.OPENDOC_PROVISIONED_FONTS = true;
  });
  await page.route(CJK_ROUTE, (route) => route.abort());
  await page.route("**/cdn.jsdelivr.net/**", (route) => route.abort());

  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.insertText("中文字");
  await expect(page.locator("#a11yDocument")).toContainText("中文字");

  await expect
    .poll(() => requests.filter((url) => CJK_FACE.test(url)).length, {
      timeout: 15_000,
    })
    .toBeGreaterThan(0);

  // This is the local-first guarantee, made reachable rather than aspirational:
  // the face is asked of US first and the third party is not the first resort.
  const cjk = requests.filter((url) => CJK_FACE.test(url));
  expect(cjk[0]).toMatch(OUR_ORIGIN_FONTS);
  expect(cjk[0]).not.toMatch(THIRD_PARTY);
});

test("a CJK face that cannot be fetched is reported, not silently drawn as boxes", async ({
  page,
}) => {
  await page.route(CJK_ROUTE, (route) => route.abort());
  await page.route("**/cdn.jsdelivr.net/**", (route) => route.abort());

  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.insertText("中文字");

  // Local-first means the editor keeps working with no network. It does not mean
  // it may show the reader a row of boxes and say nothing: a refusal has to be
  // able to say what it refused. The status line names the face and says what
  // the reader will see instead.
  await expect(page.locator("#status")).toContainText(/could not load/i, {
    timeout: 20_000,
  });
  await expect(page.locator("#status")).toContainText("▯");

  // And the editor is still usable — a font outage must not cost the document.
  await page.keyboard.insertText("Z");
  await expect(page.locator("#a11yDocument")).toContainText("Z");
});

test("a Latin-only document never asks for a script fallback face at all", async ({
  page,
  consoleErrors,
}) => {
  const fontRequests = [];
  page.on("request", (request) => {
    if (OUR_ORIGIN_FONTS.test(request.url())) fontRequests.push(request.url());
  });

  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.insertText("Plain latin text.");
  await expect(page.locator("#a11yDocument")).toContainText("Plain latin text.");

  // The named Latin faces are provisioned eagerly and legitimately; no SCRIPT
  // fallback face may be. A document that pays 16 MB for text it does not
  // contain is the other half of getting provisioning wrong.
  const scriptFallbacks = fontRequests.filter((url) =>
    /NotoSans(CJK|Arabic|Devanagari|Bengali|Thai|Hebrew|Symbols2)/.test(url),
  );
  expect(scriptFallbacks).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

// The whole chain, for real, with no network and nothing mocked: a COMMITTED
// face.
//
// Every test above fails the font request on purpose, so not one of them proves
// that a face actually arrives, verifies, registers and repaints. Proving that
// needs a face present on this origin in every environment, CI included, and a
// committed one is exactly that. `☐` (U+2610) is covered by no bundled face and
// routes to the committed `symbols` bucket, so inserting one exercises the
// complete path:
//
//   missingCoverage() → fallbackKeysFor → fetch from OUR origin → SHA-256
//   verify → registerFallbackFont → re-paginate → repaint
//
// If any link in that breaks, either the request never happens or verification
// throws and the status line says "Could not load". Both are asserted here, so
// the test cannot pass by the chain quietly not running.
test("a committed face arrives from our own origin, verifies, and registers", async ({
  page,
  consoleErrors,
}) => {
  const responses = [];
  page.on("response", (response) => {
    if (/NotoSansSymbols2-Regular\.ttf$/.test(response.url())) {
      responses.push({ url: response.url(), status: response.status() });
    }
  });

  await gotoEditor(page);
  await clickIntoFirstPage(page);
  expect(responses).toEqual([]); // coverage-driven: not yet needed

  await page.keyboard.insertText("☐");
  await expect(page.locator("#a11yDocument")).toContainText("☐");

  await expect
    .poll(() => responses.length, { timeout: 20_000 })
    .toBeGreaterThan(0);
  expect(responses[0].url).toMatch(OUR_ORIGIN_FONTS);
  expect(responses[0].url).not.toMatch(THIRD_PARTY);
  // 200, not a 404 that silently fell through to the mirror. A committed face
  // has no mirror, so a 404 here would simply be tofu with no second chance.
  expect(responses[0].status).toBe(200);

  // Verification passed and registration succeeded. A SHA-256 mismatch or a
  // blob the shaper rejected surfaces as the refusal, never as silence.
  await expect(page.locator("#status")).not.toContainText(/could not load/i);
  expect(consoleErrors).toEqual([]);
});
