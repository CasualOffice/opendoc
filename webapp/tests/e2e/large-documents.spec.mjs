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

  // 1.3 million paragraphs: the size of the reported file.
  await page.locator("#file").setInputFiles(textFile(1_303_305, "40mb.docx"));

  const status = page.locator("#status");
  await expect(status).toContainText("1,303,306 paragraphs", { timeout: 60_000 });
  await expect(status).toContainText("262,144");
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
