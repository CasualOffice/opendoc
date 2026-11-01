// Editing the text in a scanned page (`docs/182` §20b, owner decision
// 2026-11-01), end to end on a SYNTHETIC scan: printed lines drawn by the
// browser into a picture-only PDF, recognised by the committed PP-OCRv5 models
// in the OCR worker.
//
// The guarantees asserted are the user's, read off the painted page:
//   * the scan opens as its own pixels (each printed line's ink is there) and
//     its recognised text is in the document (Find finds it);
//   * deleting one line's text erases that line's original glyphs from the
//     page, and leaves the line under it as it was;
//   * saved as DOCX and opened again, the erased line stays erased and the
//     other line is still the scan's.
//
// With `IMGTEXT_SHOTS=<dir>` it also writes the owner's review screenshots
// there (synthetic only): the scan opened, a line being edited, each fill, in
// the light and the dark theme.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { MOD, clickIntoFirstPage, expect, gotoEditor, runAppMenuCommand, runPaletteCommand, test } from "./fixtures.mjs";
import { printedPage, scanPdf } from "./pdf-import-helpers.mjs";

const MODELS = fileURLToPath(new URL("../../../docs/assets/ocr/", import.meta.url));
const SHOTS = process.env.IMGTEXT_SHOTS || "";
const WIDTH = 850;
const HEIGHT = 1100;

/** The committed models under `/ocr-real/`, on the context so the worker's
 *  requests are answered too. */
async function serveModels(page) {
  const file = (name) => {
    const bytes = readFileSync(`${MODELS}${name}`);
    return { url: name, sha256: createHash("sha256").update(bytes).digest("hex"), bytes: bytes.length };
  };
  const manifest = {
    name: "PP-OCRv5 mobile",
    detection: file("det.onnx"),
    packs: [{ name: "latin", model: file("latin_rec.onnx"), dictionary: file("latin_rec.dict.txt") }],
  };
  await page.context().route(
    (url) => url.pathname === "/ocr-real/manifest.json",
    (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify(manifest) }),
  );
  await page.context().route(
    (url) => /^\/ocr-real\/[^/]+\.(onnx|txt)$/.test(url.pathname),
    (route) =>
      route
        .fulfill({ contentType: "application/octet-stream", body: readFileSync(`${MODELS}${new URL(route.request().url()).pathname.split("/").pop()}`) })
        .catch(() => {}),
  );
}

/** Printed line `i`'s ink band on page 1 as fractions of the page: the
 *  baseline `printedPage` drew it on, less the cap height, plus descenders. */
const band = (i) => ({ x0: 60 / WIDTH, x1: 780 / WIDTH, y0: (120 + i * 70 - 28) / HEIGHT, y1: (120 + i * 70 + 9) / HEIGHT });

/** Dark pixels (luma under 128) the page-1 canvases paint inside `b`. */
async function darkPixels(page, b) {
  return page.evaluate((b) => {
    const sheet = document.querySelector('.page-wrap[data-page-number="1"]');
    if (!sheet) return -1;
    const s = sheet.getBoundingClientRect();
    const region = { left: s.left + b.x0 * s.width, right: s.left + b.x1 * s.width, top: s.top + b.y0 * s.height, bottom: s.top + b.y1 * s.height };
    let dark = 0;
    for (const canvas of sheet.querySelectorAll("canvas")) {
      const r = canvas.getBoundingClientRect();
      const left = Math.max(r.left, region.left);
      const right = Math.min(r.right, region.right);
      const top = Math.max(r.top, region.top);
      const bottom = Math.min(r.bottom, region.bottom);
      if (right <= left || bottom <= top) continue;
      const sx = canvas.width / r.width;
      const sy = canvas.height / r.height;
      const g = canvas.getContext("2d", { willReadFrequently: true });
      if (!g) continue;
      const x = Math.floor((left - r.left) * sx);
      const y = Math.floor((top - r.top) * sy);
      const w = Math.max(1, Math.floor((right - left) * sx));
      const h = Math.max(1, Math.floor((bottom - top) * sy));
      const data = g.getImageData(x, y, w, h).data;
      for (let i = 0; i < data.length; i += 4) {
        if (data[i + 3] > 0 && 0.299 * data[i] + 0.587 * data[i + 1] + 0.114 * data[i + 2] < 128) dark += 1;
      }
    }
    return dark;
  }, b);
}

async function findAndClose(page, query) {
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+f`);
  await expect(page.locator("#findInput")).toBeFocused();
  await page.locator("#findInput").fill(query);
  await expect(page.locator("#findStatus")).toContainText(/1 match/, { timeout: 15_000 });
  await page.keyboard.press("Escape");
}

async function shot(page, name) {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  await page.locator('.page-wrap[data-page-number="1"]').screenshot({ path: `${SHOTS}/${name}.png` });
}

/** Opens the synthetic scan and waits for its recognised document. */
async function openScan(page) {
  await serveModels(page);
  await gotoEditor(page, "&pdfOcrModels=/ocr-real/manifest.json");
  const pdf = scanPdf({ pages: 1, width: WIDTH, height: HEIGHT, pixels: await printedPage(page, WIDTH, HEIGHT) });
  await page.locator("#file").setInputFiles({ name: "synthetic-scan.pdf", mimeType: "application/pdf", buffer: pdf });
  await expect(page.locator("#pdfImportDialog")).toHaveAttribute("data-mode", "ocr", { timeout: 60_000 });
  await expect(page.locator("#pdfImportDialog")).toBeHidden({ timeout: 200_000 });
  await expect(page.locator("#a11yDocument")).toContainText(/budget/i, { timeout: 30_000 });
  // The import summary, as after every PDF open.
  await expect(page.locator("#compatibilityFindingsDialog")).toBeVisible();
  await page.locator("#compatibilityFindingsDialogDone").click();
  await expect(page.locator("#compatibilityFindingsDialog")).toBeHidden();
  await expect.poll(() => darkPixels(page, band(2)), { timeout: 30_000 }).toBeGreaterThan(200);
}

test.describe("editing the text in a scanned page", () => {
  test.describe.configure({ timeout: 300_000 });

  test("a scan opens as its pixels; deleting a line erases its glyphs and leaves the others; DOCX keeps it", async ({ page }) => {
    await openScan(page);
    const edited = await darkPixels(page, band(2));
    const other = await darkPixels(page, band(3));
    expect(other, "the line under it is the scan's ink").toBeGreaterThan(200);
    await shot(page, "1-scan-opened");

    // The recognised line is text: select it in its line box and delete it.
    await findAndClose(page, "budget");
    await page.keyboard.press("Home");
    await page.keyboard.press("Shift+End");
    await page.keyboard.press("Backspace");
    await shot(page, "1b-line-deleted");
    await expect
      .poll(() => darkPixels(page, band(2)), { message: "the deleted line's original glyphs are still painted", timeout: 15_000 })
      .toBeLessThan(edited * 0.03);
    const after = await darkPixels(page, band(3));
    expect(Math.abs(after - other) / other, "the line under the edit changed").toBeLessThan(0.05);

    // Saved as DOCX and opened again: still erased, the other line still there.
    const download = page.waitForEvent("download");
    await runAppMenuCommand(page, "file", "file.export.docx");
    const saved = await (await download).path();
    await page.locator("#file").setInputFiles({ name: "synthetic-scan.docx", mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document", buffer: readFileSync(saved) });
    await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe("synthetic-scan.docx");
    await expect.poll(() => darkPixels(page, band(3)), { timeout: 30_000 }).toBeGreaterThan(other * 0.95);
    expect(await darkPixels(page, band(2)), "the erased line came back on reopen").toBeLessThan(edited * 0.03);
  });

  test("review screenshots: a line being edited, and each fill, light and dark", async ({ page }) => {
    test.skip(!SHOTS, "IMGTEXT_SHOTS names no directory");
    for (const scheme of ["light", "dark"]) {
      await page.emulateMedia({ colorScheme: scheme });
      await openScan(page);
      await shot(page, `2-${scheme}-scan-opened`);
      await findAndClose(page, "budget");
      await page.keyboard.press("End");
      await page.keyboard.type(" for 2027");
      await shot(page, `3-${scheme}-line-edited-fill`);
      for (const mode of ["blur", "pixelate", "fill"]) {
        await runPaletteCommand(page, `imageText.${mode}`, mode === "fill" ? "erase text under edits: fill" : mode);
        await page.waitForTimeout(300);
        await shot(page, `4-${scheme}-${mode}`);
      }
    }
  });
});
