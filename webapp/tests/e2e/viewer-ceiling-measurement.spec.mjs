// The committed probe behind `MAX_VIEWER_BLOCKS` (`crates/casual-doc-wasm`).
//
// `docs/111` §6 and the repository's evidence rule: a published number is
// generated from a committed artifact or it is not published. The constant's
// doc comment carries a table of open times and wasm linear-memory highs by
// block count; this is the thing that produces that table, so the next person
// to move the constant re-runs it instead of reasoning about it.
//
// It is **not** part of the normal suite: the largest row opens 1.3 million
// paragraphs and takes minutes. Run it deliberately:
//
//     cd webapp && ./build.sh
//     MEASURE_VIEWER_CEILING=1 npx playwright test viewer-ceiling-measurement
//
// and override the sizes with a comma-separated list if you want a different
// sweep:
//
//     MEASURE_VIEWER_CEILING=1 VIEWER_CEILING_BLOCKS=262145,600000 npx playwright test viewer-ceiling-measurement
//
// What is measured, and why each one:
//
// - **open** — wall time from handing the file to the picker to the editor
//   reporting the document open. This is the "patience ceiling" half: it is
//   linear in blocks and it is `docs/104` HF-077, not something windowing
//   fixes (`docs/113` §6.4 measured the shaping pass as unchanged).
// - **wasm memory** — `WebAssembly.Memory.buffer.byteLength` after the open.
//   Linear memory never shrinks, so this reading IS the high-water mark, and
//   it is the number that decides whether a document opens at all: past a
//   wasm32 address space the module aborts (`RuntimeError: unreachable`,
//   `docs/104` HF-158) instead of refusing honestly.
// - **JS heap** — `performance.memory.usedJSHeapSize`, for continuity with the
//   table this replaces.
// A real file can be measured instead of the synthetic shape, without it ever
// entering the repository:
//
//     MEASURE_VIEWER_CEILING=1 VIEWER_CEILING_FILE=~/Downloads/40mb.docx \
//       npx playwright test viewer-ceiling-measurement
import { readFileSync } from "node:fs";
import { basename } from "node:path";
import { test, expect, gotoEditor } from "./fixtures.mjs";

const LINE = "examplefile.com - Sample Files\r\n"; // 32 bytes, the owner's own line

const DEFAULT_BLOCKS = [262_145, 600_000, 1_303_306];

const blocks = (process.env.VIEWER_CEILING_BLOCKS ?? "")
  .split(",")
  .map((value) => Number(value.trim()))
  .filter((value) => Number.isFinite(value) && value > 1);

const sizes = blocks.length > 0 ? blocks : DEFAULT_BLOCKS;

/** The real file named by `VIEWER_CEILING_FILE`, if one was. */
function realFile() {
  const path = process.env.VIEWER_CEILING_FILE;
  if (!path) return null;
  const buffer = readFileSync(path.replace(/^~/, process.env.HOME ?? "~"));
  // The paragraph count the importer will report: one per newline, plus the
  // trailing one. Counted here so the settle condition knows what to wait for
  // without the test having been told.
  let newlines = 0;
  for (const byte of buffer) if (byte === 0x0a) newlines += 1;
  return { name: basename(path), buffer, paragraphs: newlines + 1 };
}

/** The engine's wasm linear memory, in bytes. */
async function wasmBytes(page) {
  return page.evaluate(async () => {
    // The app already initialized this exact module URL, so `default()`
    // short-circuits and hands back the SAME instance's exports — this does
    // not instantiate a second engine.
    const module = await import("/pkg/casual_doc_wasm.js");
    const wasm = await module.default();
    return wasm.memory.buffer.byteLength;
  });
}

const mib = (bytes) => `${(bytes / (1024 * 1024)).toFixed(0)} MB`;

test.describe("viewer ceiling measurement", () => {
  test.skip(
    !process.env.MEASURE_VIEWER_CEILING,
    "set MEASURE_VIEWER_CEILING=1 to run; the largest row takes minutes",
  );
  // The 1.3M row is minutes of shaping, single-threaded, by design.
  test.setTimeout(45 * 60_000);

  const real = realFile();
  const rows = real
    ? [real]
    : sizes.map((paragraphs) => ({
        // `paragraphs - 1` newline-terminated lines are `paragraphs`
        // paragraphs: the trailing empty one counts, exactly as the importer
        // counts it.
        name: "measured.docx",
        buffer: Buffer.from(LINE.repeat(paragraphs - 1)),
        paragraphs,
      }));

  for (const row of rows) {
    const { name, buffer, paragraphs } = row;
    test(`opening ${paragraphs.toLocaleString("en-US")} blocks`, async ({ page }) => {
      const crashes = [];
      const logged = [];
      page.on("pageerror", (error) => crashes.push(String(error)));
      page.on("console", (message) => {
        if (message.type() === "error") logged.push(message.text());
      });
      await gotoEditor(page);
      const baseline = await wasmBytes(page);

      const started = Date.now();
      await page.locator("#file").setInputFiles({
        name,
        mimeType: "text/plain",
        buffer,
      });

      // Either the document opens, or the viewer refuses it. Both are results.
      // `#status` also carries transient progress text, so only the error
      // class counts as a refusal — reading any non-empty status as one
      // reported a document that had in fact opened with 11,765 pages as
      // "refused", which is the kind of measurement that gets published.
      const settled = await page
        .waitForFunction(
          (want) => {
            const status = document.getElementById("status");
            if (status?.classList.contains("error")) return "refused";
            const stat = document.getElementById("statParas");
            const count = Number((stat?.textContent ?? "").replace(/[^0-9]/g, ""));
            return count === want ? "opened" : false;
          },
          paragraphs,
          { timeout: 40 * 60_000 },
        )
        .then((handle) => handle.jsonValue());
      const elapsed = (Date.now() - started) / 1000;

      const after = await wasmBytes(page);
      const heap = await page.evaluate(() => performance.memory?.usedJSHeapSize ?? 0);
      const status = (await page.locator("#status").textContent()) ?? "";
      const pages = await page.locator("#statPages").textContent();

      // Scrolling deep into the document is the other half of "it opens": a
      // windowed body has to build a window there, and a page the user
      // scrolled to that paints nothing is the failure `docs/113` §8 is about.
      let scroll = "not measured";
      let reachedLastPage = null;
      if (settled === "opened") {
        // The LAST page, not a fraction of the way in: "the document opens" has
        // to mean every page of it is reachable. A browser stops scrolling at
        // 2^24 CSS px, so a long enough document has pages the host cannot
        // reach however well the engine lays them out, and a 75%-of-the-way
        // probe would have reported that document as fine.
        const total = await page.locator(".page-wrap").count();
        const target = total - 1;
        const wrap = page.locator(".page-wrap").nth(target);
        const jumped = Date.now();
        await wrap.scrollIntoViewIfNeeded();
        // NOT a hard failure: an unreachable last page is a RESULT this probe
        // exists to record, and throwing here would suppress the row — which is
        // how a memory figure that was never printed could end up in a doc
        // comment. The row is printed either way and the assertion comes after.
        let reached = true;
        try {
          await wrap.locator("canvas").waitFor({ state: "attached", timeout: 90_000 });
        } catch {
          reached = false;
        }
        if (!reached) {
          const where = await page.evaluate((index) => {
            const viewport = document.getElementById("pages");
            const wraps = document.querySelectorAll(".page-wrap");
            const rect = wraps[index]?.getBoundingClientRect();
            return {
              canvases: document.querySelectorAll("canvas.page").length,
              wraps: wraps.length,
              // The browser stops scrolling at 2^24 = 16,777,216 CSS px, so a
              // scrollHeight past that has pages no scroll can reach.
              scrollHeight: viewport?.scrollHeight ?? -1,
              lastPageTop: Math.round(rect?.top ?? NaN),
            };
          }, target);
          scroll = `page ${target + 1} NOT REACHED in ${(
            (Date.now() - jumped) / 1000
          ).toFixed(1)} s ${JSON.stringify(where)}`;
          for (const line of logged) console.log(`|   console: ${line}`);
        } else {
          // A canvas that exists but is blank would still satisfy the wait, so
          // require ink: at least one non-background pixel.
          const inked = await wrap.locator("canvas").evaluate((canvas) => {
            const context = canvas.getContext("2d", { willReadFrequently: true });
            const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
            for (let i = 0; i < data.length; i += 4) {
              if (data[i] !== 255 || data[i + 1] !== 255 || data[i + 2] !== 255) return true;
            }
            return false;
          });
          scroll = `page ${target + 1} in ${((Date.now() - jumped) / 1000).toFixed(1)} s, ${
            inked ? "inked" : "BLANK"
          }`;
        }
        reachedLastPage = reached;
      }

      // The row. Printed rather than asserted: this test exists to produce the
      // number, and a threshold baked in here would be the number claiming to
      // measure itself.
      console.log(
        [
          `| ${paragraphs.toLocaleString("en-US")}`,
          settled,
          `${elapsed.toFixed(1)} s`,
          mib(after),
          `(+${mib(after - baseline)})`,
          `heap ${mib(heap)}`,
          `pages ${(pages ?? "").trim()}`,
          `scroll ${scroll}`,
          status.trim() ? `status: ${status.trim()}` : "",
        ].join(" | "),
      );

      // Whatever the outcome, the module must not have died: an abort is the
      // regression HF-158 exists for, and it is the one result that is not an
      // answer.
      expect(crashes, "the module must not abort").toEqual([]);
      expect(status).not.toMatch(/unreachable/i);
      // Asserted last, after the row is on the record: a size whose last page
      // cannot be reached is above the ceiling, which is the answer this probe
      // is for.
      if (settled === "opened") {
        expect(reachedLastPage, `the last page of ${paragraphs} blocks`).toBe(true);
      }
    });
  }
});
