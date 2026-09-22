// docs/104 HF-158 — opening a large document must not break the editor.
//
// A 40 MB file (plain text renamed `.docx`) spent 17 seconds being imported and
// then aborted the WebAssembly module: `RuntimeError: unreachable`, with the
// word "unreachable" shown to the user. The cause was admission limits sized for
// a 64-bit native host, plus an accessibility mirror that projected EVERY block
// of the document into the DOM — 87% of the time to open a large file, and the
// memory that killed the tab.
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { test, expect, documentPageCount, gotoEditor, pageSheet, MOD } from "./fixtures.mjs";

const LINE = "examplefile.com - Sample Files\r\n"; // 32 bytes, as in the report

/** Text of `paragraphs` lines, as a File the picker accepts. */
function textFile(paragraphs, name = "big.txt") {
  return {
    name,
    mimeType: "text/plain",
    buffer: Buffer.from(LINE.repeat(paragraphs)),
  };
}

/** The same, as a real file on disk. Past the ceiling the buffer is 57 MB and
 *  Playwright refuses to marshal more than 50 MB inline; the picker reads a
 *  file either way, which is the path under test. Returns its path, and the
 *  directory to remove afterwards. */
function textFileOnDisk(paragraphs, name = "big.txt") {
  const directory = mkdtempSync(join(tmpdir(), "opendoc-large-"));
  const path = join(directory, name);
  writeFileSync(path, LINE.repeat(paragraphs));
  return { path, directory };
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
  // plus the host's page band (`docs/113` §8.6): measured at 32.4 s and
  // 2,503 MB of wasm linear memory for 25,556 pages, every one of them
  // reachable. The refusal still has to exist and still has to be readable, so
  // this exercises it at the size where it now applies — 57 MB of text, which
  // only reaches the picker as a file on disk.
  const oversized = textFileOnDisk(1_800_001, "40mb.txt");
  try {
    await page.locator("#file").setInputFiles(oversized.path);
  } finally {
    rmSync(oversized.directory, { recursive: true, force: true });
  }

  const status = page.locator("#status");
  await expect(status).toContainText("1,800,002 paragraphs", { timeout: 120_000 });
  await expect(status).toContainText("1,800,000");
  await expect(status).toContainText("Split it into smaller documents");
  // The message must never show a trap name, and the module must not have died.
  await expect(status).not.toContainText(/unreachable/i);
  expect(crashes, "the module must not abort").toEqual([]);
  // A refusal is an expected answer, not a fault. Logging it as a console error
  // made a handled case look like a crash in every log and every spec.
  expect(consoleErrors, "a refusal must not be logged as an error").toEqual([]);

  // The document that was already open is untouched and still editable.
  expect(await paragraphCount(page)).toBe(before);
  await page.locator('.page-wrap[data-page-number="1"]').click({ position: { x: 120, y: 120 } });
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

  // A document this size opens on a measured PREFIX, so the total starts as an
  // estimate and is marked one (`~`). It must CONVERGE: the rest is measured
  // from idle time between frames (`docs/116` §7), and until it has, "the last
  // page" names a page that does not exist yet. Waiting for the marker to go
  // is therefore both the precondition for the rest of this test and the
  // assertion that the background measure actually runs and actually finishes
  // — without it the count stays approximate forever and the end of the
  // document is unreachable.
  await expect
    .poll(async () => (await page.locator("#statPages").textContent()) ?? "", {
      timeout: 4 * 60_000,
    })
    .not.toContain("~");

  // The page count is the DOCUMENT's, not the resident window's and not the
  // number of sheets on screen. A windowed body that reported its window here
  // would say five; a host that counted its own sheets would say two.
  const total = await documentPageCount(page);
  expect(total, "every page of the document must be reachable for the host").toBeGreaterThan(5_000);
  expect(
    await page.locator(".page-wrap").count(),
    "the sheets are a window on the document, not a copy of it",
  ).toBeLessThan(20);

  // The last page — thousands of pages outside the window the document opened
  // with — must paint, and paint ink.
  const last = await pageSheet(page, total);
  await last.locator("canvas").waitFor({ state: "attached", timeout: 120_000 });
  const inked = await last.locator("canvas").evaluate((canvas) => {
    const context = canvas.getContext("2d", { willReadFrequently: true });
    const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
    for (let i = 0; i < data.length; i += 4) {
      if (data[i] !== 255 || data[i + 1] !== 255 || data[i + 2] !== 255) return true;
    }
    return false;
  });
  expect(inked, `page ${total} painted nothing`).toBe(true);
  expect(crashes).toEqual([]);

  // docs/113 §8.3/§8.5 — a windowed document is READ-ONLY, because
  // re-paginating after a keystroke is the peak this path exists to avoid. The
  // engine refuses every edit at its atomic choke point; what the host owes is
  // to SAY so up front and disable what cannot work, rather than offer live
  // controls that refuse when pressed (`SKILL.md` §10).
  const banner = page.locator("#viewingBanner");
  await expect(banner).toBeVisible();
  await expect(banner).toContainText("one page-window at a time");
  await expect(banner).toContainText("more than 262,144 paragraphs");
  // There is nothing to switch to, so the escape hatch is not offered.
  await expect(page.locator("#viewingBannerEdit")).toBeHidden();
  // Editing and Suggesting cannot be chosen at all, and say why on hover.
  for (const mode of ["editing", "suggesting", "viewing"]) {
    const buttons = page.locator(`[data-review-mode="${mode}"]`);
    for (let i = 0; i < (await buttons.count()); i++) {
      await expect(buttons.nth(i)).toBeDisabled();
      await expect(buttons.nth(i)).toHaveAttribute("title", /page-window at a time/);
    }
  }

  // And typing says the same thing. "QZX" rather than any marker containing
  // "@": the corpus has e-mail addresses in it and every search matches those.
  const before = await page.locator("#a11yDocument").textContent();
  await last.locator("canvas").click({ position: { x: 120, y: 120 } });
  await page.keyboard.type("QZX");
  const status = page.locator("#status");
  await expect(status).toHaveClass(/error/);
  await expect(status).toContainText("one page-window at a time");
  // The message the host used to show for this — the one `edit_errors.mjs`
  // gives every refusal it has nothing better to say about — sent the reader
  // to inspect a selection that is perfectly fine.
  await expect(status).not.toContainText("selection");
  // No silent half-edit: the document is exactly what it was.
  expect(await page.locator("#a11yDocument").textContent()).toBe(before);
});

test("replacing a document that is still being measured does not reach into freed memory", async ({
  page,
  consoleErrors,
}) => {
  // `docs/116` §7. A document opened on a prefix keeps measuring the rest from
  // idle time, and those ticks call the engine. Open another document and the
  // wrapper they call is freed — `openBytes` frees it the moment the new one
  // has parsed. A tick that survives that crosses into freed memory, which is
  // the "null pointer passed to rust" the open path already guards against for
  // every other late caller.
  //
  // This is a REGRESSION SMOKE TEST, not a proof. The window it aims at is a
  // race — an idle tick landing between `free()` and the new document arming
  // its own ticker — and removing the guards does not reliably reproduce it
  // from out here (measured: the mutation passed). What makes the crash
  // impossible is the stop at the free itself, which is deterministic; this
  // asserts the observable half, that replacing a still-measuring document is
  // clean and the new count is its own. The ticker's stop semantics are
  // covered directly in `background_measure.test.mjs`.
  // Past MAX_WHOLE_LAYOUT_BLOCKS (262,144), because that is the only path that
  // opens on a prefix: a document laid out whole is measured whole and its
  // count is exact from the first frame, so a smaller file would leave nothing
  // still measuring and this would pass without testing anything.
  test.setTimeout(6 * 60_000);
  const crashes = [];
  page.on("pageerror", (error) => crashes.push(String(error)));
  await gotoEditor(page);

  await page.locator("#file").setInputFiles(textFile(262_145, "measuring.docx"));
  await expect(page.locator("#statPages")).toContainText("~", { timeout: 5 * 60_000 });

  await page.locator("#file").setInputFiles(textFile(40, "small.docx"));
  await expect.poll(() => paragraphCount(page), { timeout: 60_000 }).toBe(41);

  // Give the abandoned ticker every chance to fire into the freed wrapper.
  await page.waitForTimeout(2_000);
  expect(crashes, "a late measure tick must not touch the freed document").toEqual([]);
  expect(consoleErrors).toEqual([]);

  // ...and the small document's own count is its own — exact, no marker.
  await expect(page.locator("#statPages")).not.toContainText("~");
});
