// A scanned PDF is recognised on the editor as the Pages build deploys it
// (HF-414; owner report 2026-10-31, "OCR works is still not there").
//
// What the deployment does, reproduced here: the editor page carries the
// `<meta name="opendoc-ocr-manifest" content="./ocr/manifest.json">` tag that
// `tools/stage-ocr-models.mjs` writes, the manifest and the committed PP-OCRv5
// models are served same-origin under `/ocr/` with file-relative URLs, and no
// host option names them. And the page is NOT a secure context:
// `http://opendoc.casualoffice.org/editor.html` answers over plain HTTP without
// redirecting, so it has no `crypto.subtle` and no `crypto.randomUUID`. On the
// live site over HTTP every open threw "globalThis.crypto.randomUUID is not a
// function", and the models could never pass their SHA-256 check. A host name
// mapped to the loopback address (`--host-resolver-rules`) gives that origin
// here: Chromium treats `127.0.0.1` and `localhost` as secure, any other name
// over HTTP as not.
//
// The guarantee asserted is the user's: the scan opens, recognition runs on its
// own, and the printed words become document text a search finds.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { MOD, clickIntoFirstPage, expect, test } from "./fixtures.mjs";
import { printedPage, scanPdf } from "./pdf-import-helpers.mjs";

const MODELS = fileURLToPath(new URL("../../../docs/assets/ocr/", import.meta.url));
const SPEC = JSON.parse(readFileSync(new URL("../../../tools/ocr-models.json", import.meta.url), "utf8"));
const HOST = "opendoc-insecure.test";

test.use({
  launchOptions: {
    args: ["--enable-precise-memory-info", "--js-flags=--expose-gc", `--host-resolver-rules=MAP ${HOST} 127.0.0.1`],
  },
});

/** The deployment's `/ocr/`: the manifest `stage-ocr-models.mjs` writes, from
 *  the same `tools/ocr-models.json`, and the committed models. */
async function serveDeployedModels(page) {
  const entry = (spec) => ({ url: spec.file, sha256: spec.sha256, bytes: spec.bytes });
  const manifest = {
    name: SPEC.name,
    detection: entry(SPEC.detection),
    packs: SPEC.packs.map((pack) => ({ name: pack.name, model: entry(pack.model), dictionary: entry(pack.dictionary) })),
  };
  await page.context().route(
    (url) => url.pathname === "/ocr/manifest.json",
    (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify(manifest) }),
  );
  await page.context().route(
    (url) => /^\/ocr\/[^/]+\.(onnx|txt)$/.test(url.pathname),
    (route) => {
      const name = new URL(route.request().url()).pathname.split("/").pop();
      return route.fulfill({ contentType: "application/octet-stream", body: readFileSync(`${MODELS}${name}`) });
    },
  );
  // The tag the Pages build adds to editor.html. Added as the page parses, not
  // by rewriting the response: a page fulfilled by the runner loses its
  // loopback address space, and Private Network Access then blocks every
  // script it loads from 127.0.0.1. The editor reads the tag only when a PDF
  // opens (`ocrManifestUrl`), long after this has run.
  await page.addInitScript(() => {
    document.addEventListener("DOMContentLoaded", () => {
      const meta = document.createElement("meta");
      meta.name = "opendoc-ocr-manifest";
      meta.content = "./ocr/manifest.json";
      document.head.append(meta);
    });
  });
}

test.describe("scanned-PDF recognition as deployed", () => {
  test.describe.configure({ timeout: 240_000 });

  test("over plain HTTP, a scanned PDF opens and its printed words become searchable text on their own", async ({ page, baseURL }) => {
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await serveDeployedModels(page);
    const port = new URL(baseURL).port;
    await page.goto(`http://${HOST}:${port}/editor.html?fixture=rich`);
    expect(await page.evaluate(() => window.isSecureContext), "the page must be an insecure context, as plain HTTP is").toBe(false);
    await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0 && document.body.dataset.fontsReady === "true", null, {
      timeout: 60_000,
    });
    await expect(page.locator("#status")).not.toHaveClass(/error/);
    expect(await page.evaluate(() => document.querySelector('meta[name="opendoc-ocr-manifest"]')?.content)).toBe("./ocr/manifest.json");

    const width = 850;
    const height = 1100;
    const pdf = scanPdf({ pages: 1, width, height, pixels: await printedPage(page, width, height) });
    await page.locator("#file").setInputFiles({ name: "printed-scan.pdf", mimeType: "application/pdf", buffer: pdf });
    await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe("printed-scan.pdf");

    // Recognition starts by itself, with the deployment's models, in the
    // background: no dialog, progress in the status bar.
    const running = page.locator("body[data-pdf-recognition=running]");
    await expect(running).toHaveCount(1, { timeout: 60_000 });
    await expect(page.locator("#pdfImportDialog")).toBeHidden();
    await expect(running).toHaveCount(0, { timeout: 200_000 });
    await expect(page.locator("#status")).not.toHaveClass(/error/);

    // The words are text now: in the document, and found by Find.
    await expect(page.locator("#a11yDocument")).toContainText(/budget/i, { timeout: 30_000 });
    // The recognised document's import summary, as after every PDF open.
    const findings = page.locator("#compatibilityFindingsDialog");
    await expect(findings).toBeVisible();
    await page.locator("#compatibilityFindingsDialogDone").click();
    await expect(findings).toBeHidden();
    await clickIntoFirstPage(page);
    await page.keyboard.press(`${MOD}+f`);
    const findInput = page.locator("#findInput");
    await expect(findInput).toBeFocused();
    await findInput.pressSequentially("budget");
    await expect(page.locator("#findStatus")).toHaveText(/^(1 match|1 of [1-9]\d*)$/);
    expect(errors, "no uncaught error on the way").toEqual([]);
  });
});
