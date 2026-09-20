// docs/104 HF-158 — opening a large document must not break the editor.
//
// A 40 MB file (plain text renamed `.docx`) spent 17 seconds being imported and
// then aborted the WebAssembly module: `RuntimeError: unreachable`, with the
// word "unreachable" shown to the user. The cause was admission limits sized for
// a 64-bit native host, plus an accessibility mirror that projected EVERY block
// of the document into the DOM — 87% of the time to open a large file, and the
// memory that killed the tab.
import { test, expect, gotoEditor, MOD } from "./fixtures.mjs";

const LINE = "examplefile.com - Sample Files\r\n"; // 32 bytes, as in the report

/** Text of `paragraphs` lines, as a File the picker accepts. */
function textFile(paragraphs, name = "big.txt") {
  return {
    name,
    mimeType: "text/plain",
    buffer: Buffer.from(LINE.repeat(paragraphs)),
  };
}

async function paragraphCount(page) {
  const text = await page.locator("#statParas").textContent();
  return Number(text.replace(/[^0-9]/g, ""));
}

test("a document far past the ceiling is refused with a reason, and the editor survives", async ({
  page,
  consoleErrors,
}) => {
  const crashes = [];
  page.on("pageerror", (error) => crashes.push(String(error)));
  await gotoEditor(page);
  const before = await paragraphCount(page);

  // One paragraph past the ceiling. It used to be 1,303,306 — the size of the
  // reported file — and that size now OPENS, through the windowed layout path
  // (`docs/113`): measured at 110.5 s and 2,476 MB of wasm linear memory for
  // 25,556 pages, against the 4.14 GiB the whole-layout path needed. The
  // refusal still has to exist and still has to be readable, so this exercises
  // it at the size where it now applies.
  await page.locator("#file").setInputFiles(textFile(700_001, "40mb.docx"));

  const status = page.locator("#status");
  await expect(status).toContainText("700,002 paragraphs", { timeout: 120_000 });
  await expect(status).toContainText("700,000");
  await expect(status).toContainText("Split it into smaller documents");
  // The message must never show a trap name, and the module must not have died.
  await expect(status).not.toContainText(/unreachable/i);
  expect(crashes, "the module must not abort").toEqual([]);
  // A refusal is an expected answer, not a fault. Logging it as a console error
  // made a handled case look like a crash in every log and every spec.
  expect(consoleErrors, "a refusal must not be logged as an error").toEqual([]);

  // The document that was already open is untouched and still editable.
  expect(await paragraphCount(page)).toBe(before);
  await page.locator(".page-wrap").first().click({ position: { x: 120, y: 120 } });
  await page.keyboard.type("STILLALIVE");
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("STILLALIVE");
  await expect(page.locator("#findStatus")).toContainText("1 match");
});

test("a large document opens, and the accessibility mirror stays a window", async ({ page }) => {
  const crashes = [];
  page.on("pageerror", (error) => crashes.push(String(error)));
  await gotoEditor(page);

  // 65,537 paragraphs — the size that used to abort the module outright.
  await page.locator("#file").setInputFiles(textFile(65_536));
  await expect(page.locator("#docTitle")).toHaveValue("big.txt", { timeout: 60_000 });
  await expect
    .poll(() => paragraphCount(page), { timeout: 60_000 })
    .toBe(65_537);
  expect(crashes).toEqual([]);

  // The mirror projects a bounded window, not one node per paragraph. Before
  // this it built 65,537 nodes and the tab ran out of memory.
  const mirrored = await page.locator("#a11yDocument > *").count();
  expect(mirrored, `mirror projected ${mirrored} nodes`).toBeLessThan(1_000);
  // And it says it is a window, so a screen reader is not told the document is
  // only as long as the part it can see.
  await expect(page.locator("#a11yDocument")).toContainText(/Showing blocks 1 to \d+ of 65,?537/);
});

// docs/113 §8 — a document too large to lay out whole opens one page-window at
// a time, and the pages outside the window are still real pages.
//
// This is the browser half of the invariant `crates/casual-doc-wasm` asserts
// natively ("a windowed page equals the same page of a whole layout, field for
// field"). What it adds is the thing only a browser can show: that the page a
// user SCROLLS TO — far outside the window the document opened with — paints
// ink rather than an empty sheet, which is the silent failure `docs/113` §8 is
// written against.
test("a document too large to lay out whole opens windowed, and its far pages still paint", async ({
  page,
}) => {
  // One block past MAX_WHOLE_LAYOUT_BLOCKS (262,144): the smallest document
  // that takes the windowed path, so this guard costs the least time that can
  // still cover it. Measured at 17-31 s to open.
  const paragraphs = 262_146;
  test.setTimeout(6 * 60_000);
  const crashes = [];
  page.on("pageerror", (error) => crashes.push(String(error)));
  await gotoEditor(page);

  await page.locator("#file").setInputFiles(textFile(paragraphs - 1, "windowed.docx"));
  await expect.poll(() => paragraphCount(page), { timeout: 5 * 60_000 }).toBe(paragraphs);
  expect(crashes, "the module must not abort").toEqual([]);

  // The page count is the DOCUMENT's, not the resident window's. A windowed
  // body that reported its window here would say five.
  const wraps = await page.locator(".page-wrap").count();
  expect(wraps, "every page of the document must exist for the host").toBeGreaterThan(5_000);

  // The last page — thousands of pages outside the window the document opened
  // with — must paint, and paint ink.
  const last = page.locator(".page-wrap").nth(wraps - 1);
  await last.scrollIntoViewIfNeeded();
  await last.locator("canvas").waitFor({ state: "attached", timeout: 120_000 });
  const inked = await last.locator("canvas").evaluate((canvas) => {
    const context = canvas.getContext("2d", { willReadFrequently: true });
    const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
    for (let i = 0; i < data.length; i += 4) {
      if (data[i] !== 255 || data[i + 1] !== 255 || data[i + 2] !== 255) return true;
    }
    return false;
  });
  expect(inked, `page ${wraps} painted nothing`).toBe(true);
  expect(crashes).toEqual([]);
});
