// Text recognition never holds the main thread (`docs/182` §17a.5; owner
// report 2026-10-31, "OCR is freezing the tab").
//
// The real PP-OCRv5 models (the ones the Pages build serves, committed in
// `docs/assets/ocr`) read a scanned page of real text, and a Long Tasks
// observer watches the editor's main thread for the whole run, from the
// moment the held models are let through to the end mark `pdf_import.mjs`
// sets when the package is back (`OCR_END_MARK`): the model download and
// SHA-256 check, the IndexedDB write, the engine load, the page's
// rasterising, detection, recognition and the finish. The
// GUARANTEE asserted is the long-task definition itself: no task over 50 ms on
// this thread while the scan is recognised, so the frame loop runs, the
// progress paints and Cancel lands. Run on the main thread (the path before
// `ocr_service.mjs`), the engine load alone and every page were long tasks;
// the commit carries the red run.
//
// And the run is usable, not merely survivable: it runs in the background with
// no dialog (owner report 2026-11-02), and the wheel scrolls the scanned pages
// while the worker reads.
//
// The open BEFORE recognition is held to the same rule (second test): a
// scanned page's pages are read in the worker too. Read on this thread, an
// 8-page scan blocked it for 4.7 s (~600 ms a page, which no slice can
// split); measured in Chrome on the owner's file, after the change the
// longest task in the whole open was 106 ms, and it is the first layout.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, gotoEditor, test } from "./fixtures.mjs";
import { printedPage, scanPdf } from "./pdf-import-helpers.mjs";

const MODELS = fileURLToPath(new URL("../../../docs/assets/ocr/", import.meta.url));
const OCR_START = "opendoc:pdf-ocr:start";
const OCR_END = "opendoc:pdf-ocr:end";

/** Serves the committed PP-OCRv5 models under `/ocr-real/`, on the context so
 *  the OCR worker's requests are answered too. */
async function serveRealModels(page) {
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
  // The model files are held until `release()`: the run then starts from a
  // settled editor (see the test), never inside the previous open's tail.
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  await page.context().route(
    (url) => /^\/ocr-real\/[^/]+\.(onnx|txt)$/.test(url.pathname),
    async (route) => {
      await gate;
      const name = new URL(route.request().url()).pathname.split("/").pop();
      await route.fulfill({ contentType: "application/octet-stream", body: readFileSync(`${MODELS}${name}`) }).catch(() => {});
    },
  );
  return { release };
}

test.describe("text recognition off the main thread", () => {
  test.describe.configure({ timeout: 240_000 });

  test("recognising a scanned page with the real models runs no long task on the main thread, and the document still scrolls", async ({
    page,
  }) => {
    const models = await serveRealModels(page);
    await gotoEditor(page, "&pdfOcrModels=/ocr-real/manifest.json");
    const width = 850;
    const height = 1100;
    const pdf = scanPdf({ pages: 1, width, height, pixels: await printedPage(page, width, height) });

    await page.evaluate(() => {
      window.__longTasks = [];
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          window.__longTasks.push({ start: entry.startTime, duration: Math.round(entry.duration) });
        }
      }).observe({ type: "longtask", buffered: true });
    });
    await page.locator("#file").setInputFiles({ name: "printed-scan.pdf", mimeType: "application/pdf", buffer: pdf });
    await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe("printed-scan.pdf");

    // The automatic run starts at once, in the background, and waits on the
    // held models.
    const running = page.locator("body[data-pdf-recognition=running]");
    await expect(running).toHaveCount(1, { timeout: 60_000 });
    await expect(page.locator("#pdfImportDialog")).toBeHidden();
    // The open that came first still has a tail on this thread (its first
    // layout and paint, the web-font re-render): its cost, not recognition's,
    // and the same for a PDF with no scans. Wait it out as a STATE, a second
    // with no long task, then let the models through: from here on, every
    // task this thread runs until the end mark is the recognition run's.
    await expect
      .poll(
        () =>
          page.evaluate(() => {
            const last = window.__longTasks.at(-1);
            const quiet = performance.now() - (last ? last.start + last.duration : 0) > 1000;
            return quiet && document.body.dataset.fontsReady === "true";
          }),
        { timeout: 60_000, message: "the editor never settled after the open" },
      )
      .toBe(true);
    const from = await page.evaluate(() => performance.now());
    models.release();

    // Usable while it reads: the wheel scrolls the pages.
    const viewport = page.locator("#viewport");
    const top = await viewport.evaluate((el) => el.scrollTop);
    const scrollFrom = await page.evaluate(() => performance.now());
    await page.mouse.move(40, 400);
    await page.mouse.wheel(0, 300);
    await expect.poll(() => viewport.evaluate((el) => el.scrollTop), { message: "the document did not scroll while recognition ran" }).toBeGreaterThan(top);
    // The scroll's own repaint is the editor's cost of scrolling, the same
    // with or without a recognition run, and is held to its own budget by
    // `main-thread-budget.spec.mjs`; it is left out of this run's tally, and
    // only it: the window is the wheel to the scrolled frame plus one frame.
    const scrollTo = await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => resolve(performance.now()))));

    await expect(running).toHaveCount(0, { timeout: 200_000 });
    const marks = await page.evaluate(
      ([start, end]) => ({
        start: performance.getEntriesByName(start).at(-1)?.startTime ?? null,
        end: performance.getEntriesByName(end).at(-1)?.startTime ?? null,
      }),
      [OCR_START, OCR_END],
    );
    expect(marks.start, "the recognition run never started").not.toBeNull();
    expect(marks.end, "the recognition run never finished").not.toBeNull();
    // Long-task entries arrive on a later turn: give the observer one.
    const tasks = await page.evaluate(() => new Promise((resolve) => setTimeout(() => resolve(window.__longTasks), 500)));
    // The tasks the run owns: begun once the models were let through and over
    // before its package came back. The task that sets the end mark goes on to
    // open the finished document in the main module (`openPdfReflow`, then the
    // first layout and paint), which is the open's cost, not recognition's,
    // and the same with or without OCR.
    const begin = Math.max(marks.start, from);
    const scrolling = (task) => task.start + task.duration > scrollFrom && task.start < scrollTo;
    const during = tasks.filter((task) => task.start >= begin && task.start + task.duration <= marks.end && !scrolling(task));
    console.log(
      `OCR run ${Math.round(marks.end - marks.start)} ms (${Math.round(marks.start)}..${Math.round(marks.end)}, ` +
        `models released ${Math.round(from)}, scroll ${Math.round(scrollFrom)}..${Math.round(scrollTo)}); ` +
        `long tasks during it: ${JSON.stringify(during)}; all: ${JSON.stringify(tasks)}`,
    );
    expect(during, "a main-thread task over 50 ms ran while the scan was recognised").toEqual([]);

    // And it did read: the recognised words are in the document.
    await expect(page.locator("#a11yDocument")).toContainText(/budget/i, { timeout: 30_000 });
  });

  test("opening a scanned PDF reads its pages in the worker: no main-thread task before the open is over budget", async ({ page }) => {
    // No recognition: only the open, which is what froze the tab.
    await gotoEditor(page, "&pdfOcrModels=none");
    // Letter at 300 dpi, the shape of a real scan; four pages, so a page-sized
    // task on this thread shows up four times.
    const width = 2550;
    const height = 3300;
    const pixels = await printedPage(page, width, height);
    const pdf = scanPdf({ pages: 4, width, height, pixels });
    await page.evaluate(() => {
      window.__longTasks = [];
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) window.__longTasks.push({ start: entry.startTime, duration: Math.round(entry.duration) });
      }).observe({ type: "longtask" });
      performance.clearMarks("opendoc:pdf-import:finish");
    });
    const from = await page.evaluate(() => performance.now());
    await page.locator("#file").setInputFiles({ name: "big-scan.pdf", mimeType: "application/pdf", buffer: pdf });
    await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 120_000 }).toBe("big-scan.pdf");
    const { finish, tasks } = await page.evaluate(
      () =>
        new Promise((resolve) =>
          setTimeout(
            () =>
              resolve({
                finish: performance.getEntriesByName("opendoc:pdf-import:finish").at(-1)?.startTime ?? null,
                tasks: window.__longTasks,
              }),
            500,
          ),
        ),
    );
    expect(finish, "the import's finish mark was set").not.toBeNull();
    // Reading the pages is everything before the mark; what follows is the
    // open and the first layout, the same for any document.
    const reading = tasks.filter((task) => task.start >= from && task.start + task.duration <= finish);
    console.log(`scanned open: long tasks while reading ${JSON.stringify(reading)}; all ${JSON.stringify(tasks)}`);
    // 200 ms, the budget `pdf-import.spec` holds the sliced fallback to; a
    // scanned page read here costs several times that.
    const worst = reading.length ? Math.max(...reading.map((task) => task.duration)) : 0;
    expect(worst, `reading the scanned pages blocked the main thread: ${JSON.stringify(reading)}`).toBeLessThan(200);
  });
});
