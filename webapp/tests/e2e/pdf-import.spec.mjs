// Opening a PDF in the editor (`docs/182` P2): the staged import behind its
// progress dialog, the reconstruction summary, editing a frame, saving as PDF
// and DOCX, Cancel, and the main thread during the import.
//
// The import's slices run in idle callbacks (`pdf_import_scheduler.mjs`). To
// SEE the dialog in a state that a fast machine would otherwise pass through in
// a frame, the specs hold idle callbacks behind a gate installed before the
// editor loads, and release them when the assertion is made: the slices are
// delayed, never skipped, so what runs is exactly what a slow device runs.
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { clickIntoFirstPage, expect, gotoEditor, runAppMenuCommand, stableBox, test } from "./fixtures.mjs";
import { PDF_FIXTURE as FIXTURE, installIdleGate, startHeldPdfImport } from "./pdf-import-helpers.mjs";
const NAME = "synthetic-prose.pdf";
const dialog = (page) => page.locator("#pdfImportDialog");
const findings = (page) => page.locator("#compatibilityFindingsDialog");

async function pageCount(page) {
  return page.locator(".page-wrap").count();
}

async function download(page, commandId) {
  const pending = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", commandId);
  const saved = await pending;
  return { name: saved.suggestedFilename(), bytes: await readFile(await saved.path()) };
}

/** Opens `bytes` as a file named `name` and waits until it is the document. */
async function openBuffer(page, name, bytes) {
  await page.locator("#file").setInputFiles({ name, mimeType: "application/octet-stream", buffer: bytes });
  await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe(name);
}

async function closeFindingsIfOpen(page) {
  if (await findings(page).isVisible()) {
    await page.locator("#compatibilityFindingsDialogDone").click();
    await expect(findings(page)).toBeHidden();
  }
}

test.describe("PDF import in the editor", () => {
  test.setTimeout(120_000);

  // `docs/182` §4: a PDF opens as a flowing document, as Word and Google Docs
  // open one. The progress dialog offers "Keep exact page layout"; left
  // unticked, what opens is body paragraphs, saved as DOCX by default, and
  // the summary says the layout was inferred. Mutation: make `layoutOf`
  // return "fixed" for `import` and the summary is `pdfDisclosure`.
  test("a PDF opens as a flowing document by default, and Keep exact page layout reopens its pages", async ({
    page,
    consoleErrors,
  }) => {
    await installIdleGate(page);
    await gotoEditor(page);
    await startHeldPdfImport(page);
    const keep = page.locator("#pdfImportDialogKeepLayout");
    await expect(keep).toBeVisible();
    await expect(keep).not.toBeChecked();
    await expect(page.locator("#pdfImportDialogKeepLayoutLabel")).toHaveText("Keep exact page layout");
    await page.evaluate(() => window.__releaseIdle());
    await expect(dialog(page)).toBeHidden({ timeout: 60_000 });
    await expect(page.locator("#docTitle")).toHaveValue(NAME);
    await expect(findings(page)).toBeVisible();
    const disclosure = findings(page).locator('[data-kind="pdfFlowingDisclosure"]');
    await expect(disclosure).toContainText("A flowing document rebuilt from a PDF");
    await expect(disclosure.locator('[data-fact="keepLayout"]')).toContainText("Keep exact page layout");
    await expect(findings(page).locator('[data-feature="pdf.import.layout_inferred"]')).toBeVisible();
    await closeFindingsIfOpen(page);
    await expect(page.locator("#a11yDocument")).toContainText("quick brown fox");
    await expect(page.locator("#saveFormat")).toHaveValue("org.openxmlformats.wordprocessingml.document");
    // Saved as PDF it is a new file, never the original written back.
    const original = await readFile(FIXTURE);
    const pdf = await download(page, "file.export.pdf");
    expect(pdf.bytes.subarray(0, 5).toString()).toBe("%PDF-");
    expect(pdf.bytes.equals(original), "a flowing document does not write the original back").toBe(false);

    // The command, from the File menu: the same PDF with its pages as drawn.
    await runAppMenuCommand(page, "file", "file.pdf.keepExactLayout");
    await expect(findings(page)).toBeVisible({ timeout: 60_000 });
    await expect(findings(page).locator('[data-kind="pdfDisclosure"]')).toContainText("This is a reconstruction of a PDF");
    await closeFindingsIfOpen(page);
    await expect.poll(() => pageCount(page)).toBe(2);
    const exact = await download(page, "file.export.pdf");
    expect(exact.bytes.equals(original), "the exact layout saves back as the original bytes").toBe(true);
    expect(consoleErrors).toEqual([]);
  });

  test("with its exact page layout kept, a PDF is summarised, edits in a frame, and saves as PDF and DOCX", async ({
    page,
    consoleErrors,
  }) => {
    await installIdleGate(page);
    await gotoEditor(page, "&pdfLayout=fixed");
    await startHeldPdfImport(page);
    await expect(page.locator("#pdfImportDialogKeepLayout")).toBeChecked();

    // The dialog, before any page: titled with the file, Cancel focused, the bar
    // determinate once admission has counted the pages.
    await expect(page.locator("#pdfImportDialogTitle")).toHaveText(`Opening ${NAME}`);
    await expect(page.locator("#pdfImportDialogCancel")).toBeFocused();
    const bar = page.locator("#pdfImportDialogBar");
    await expect(bar).toHaveAttribute("role", "progressbar");
    await expect(bar).toHaveAttribute("aria-valuemax", "2");
    await expect(bar).toHaveAttribute("aria-valuenow", "0");
    await expect(page.locator("#pdfImportDialogCount")).toHaveText("Page 0 of 2");
    // The previous document is still the document while the import runs.
    await expect(page.locator("#docTitle")).toHaveValue("opendoc-demo.docx");

    await page.evaluate(() => window.__releaseIdle());
    await expect(dialog(page)).toBeHidden({ timeout: 60_000 });
    await expect(page.locator("#docTitle")).toHaveValue(NAME);
    await expect.poll(() => pageCount(page)).toBe(2);

    // The summary opens by itself, disclosure first: local, a reconstruction,
    // the original untouched; then what the import approximated.
    await expect(findings(page)).toBeVisible();
    const disclosure = findings(page).locator('[data-kind="pdfDisclosure"]');
    await expect(disclosure).toContainText("This is a reconstruction of a PDF");
    await expect(disclosure.locator("li")).toHaveText([
      "It was converted on this device. Nothing was uploaded.",
      /rebuilt as text frames, pictures and shapes/,
      /original file is untouched/,
    ]);
    const degraded = findings(page).locator('[data-kind="pdfDegraded"]');
    await expect(degraded).toContainText("Rebuilt approximately");
    await expect(degraded.locator('[data-feature="pdf.import.font_substituted"]')).toContainText(
      "Fonts replaced by a similar font",
    );
    await closeFindingsIfOpen(page);

    // Unchanged, Save as PDF is the original file byte for byte.
    const original = await readFile(FIXTURE);
    const unchanged = await download(page, "file.export.pdf");
    expect(unchanged.bytes.equals(original), "an unchanged PDF saves back as the original bytes").toBe(true);

    // Edit a frame: Word's grammar, a double-click enters the text box.
    await expect(page.locator("body")).toHaveAttribute("data-fonts-ready", "true");
    const sheet = await stableBox(page.locator(".page-wrap .page").first());
    await page.mouse.dblclick(sheet.x + sheet.width * 0.2, sheet.y + sheet.height * 0.125);
    await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
    await page.keyboard.type("EDITED ");
    await expect(page.locator("#a11yDocument")).toContainText("EDITED");

    // Edited, Save as PDF is regenerated, and its text layer holds the edit:
    // reopened through the same importer, the frame says EDITED.
    const edited = await download(page, "file.export.pdf");
    expect(edited.bytes.subarray(0, 5).toString()).toBe("%PDF-");
    expect(edited.bytes.equals(original)).toBe(false);
    const docx = await download(page, "file.export.docx");
    expect(docx.name).toBe("synthetic-prose.docx");

    await openBuffer(page, "edited.pdf", edited.bytes);
    await closeFindingsIfOpen(page);
    await expect(page.locator("#a11yDocument")).toContainText("EDITED");
    // DOCX: the frames are text boxes Word opens, the edit inside one.
    await openBuffer(page, "edited.docx", docx.bytes);
    await expect(page.locator("#a11yDocument")).toContainText("EDITED");

    expect(consoleErrors).toEqual([]);
  });

  test("Cancel mid-import leaves the open document exactly as it was", async ({ page, consoleErrors }) => {
    await installIdleGate(page);
    await gotoEditor(page);
    const before = await page.locator("#a11yDocument").textContent();
    const pagesBefore = await pageCount(page);

    // The button.
    await startHeldPdfImport(page);
    await page.locator("#pdfImportDialogCancel").click();
    await expect(page.locator("#pdfImportDialogPhase")).toHaveText("Cancelling…");
    await page.evaluate(() => window.__releaseIdle());
    await expect(dialog(page)).toBeHidden();
    await expect(page.locator("#status")).toHaveText(
      `Opening ${NAME} was cancelled — the document you had open is still here`,
    );
    await expect(page.locator("#docTitle")).toHaveValue("opendoc-demo.docx");
    expect(await pageCount(page)).toBe(pagesBefore);
    expect(await page.locator("#a11yDocument").textContent()).toBe(before);

    // Escape is Cancel too, through the one modal contract.
    await startHeldPdfImport(page);
    await page.keyboard.press("Escape");
    await expect(dialog(page)).toBeHidden();
    await page.evaluate(() => window.__releaseIdle());
    await expect(page.locator("#status")).toContainText("was cancelled");
    await expect(page.locator("#docTitle")).toHaveValue("opendoc-demo.docx");
    expect(await page.locator("#a11yDocument").textContent()).toBe(before);
    // And the editor still edits the document it kept.
    await clickIntoFirstPage(page);
    await page.keyboard.type("kept");
    await expect(page.locator("#a11yDocument")).toContainText("kept");

    expect(consoleErrors).toEqual([]);
  });

  test("an encrypted PDF is refused in words, and the open document stays", async ({ page }) => {
    await gotoEditor(page);
    await page
      .locator("#file")
      .setInputFiles(fileURLToPath(new URL("../../../fixtures/pdf/hostile/hostile-encrypted.pdf", import.meta.url)));
    await expect(page.locator("#status")).toHaveText(
      "hostile-encrypted.pdf is password-protected. Password-protected PDFs cannot be opened yet. — the document you had open is still here",
    );
    await expect(page.locator("#docTitle")).toHaveValue("opendoc-demo.docx");
    await expect(dialog(page)).toBeHidden();
  });

  test("no main-thread task during a long PDF's import exceeds the slice budget", async ({ page, consoleErrors }) => {
    test.setTimeout(240_000);
    await gotoEditor(page);
    // A long PDF made by this editor, so it is rights-safe and its size is
    // stated here: plain text, about forty lines a page.
    const lines = Array.from({ length: 14_000 }, (_, i) => `Line ${i + 1} of a long document, written to be imported back.`);
    const scratch = await mkdtemp(join(tmpdir(), "opendoc-pdf-"));
    try {
      const text = join(scratch, "long.txt");
      await writeFile(text, lines.join("\n"));
      await page.locator("#file").setInputFiles(text);
      await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe("long.txt");
      const pdf = await download(page, "file.export.pdf");
      const pages = Number(/\/Count (\d+)/.exec(pdf.bytes.toString("latin1"))?.[1] ?? 0);
      expect(pages, "the generated PDF is long enough to need many slices").toBeGreaterThan(250);

      await page.evaluate(() => {
        window.__tasks = [];
        window.__taskObserver = new PerformanceObserver((list) => {
          for (const entry of list.getEntries()) window.__tasks.push([entry.startTime, Math.round(entry.duration)]);
        });
        window.__taskObserver.observe({ entryTypes: ["longtask"] });
        performance.clearMarks("opendoc:pdf-import:finish");
      });
      await openBuffer(page, "long.pdf", pdf.bytes);
      const { sliced, finish, slices } = await page.evaluate(
        () =>
          new Promise((resolve) =>
            setTimeout(() => {
              const mark = performance.getEntriesByName("opendoc:pdf-import:finish")[0]?.startTime ?? Infinity;
              const tasks = window.__tasks ?? [];
              resolve({
                // The mark is set INSIDE the task that runs the finish, so that
                // task starts at or a hair before it: partitioned by start
                // alone, the finish was counted as a slice whenever the two
                // fell in different 0.1 ms ticks (measured: task 9467.1, mark
                // 9467.2). A slice is a task over before the mark.
                sliced: tasks.filter(([start, duration]) => start + duration <= mark).map(([, duration]) => duration),
                finish: tasks.filter(([start, duration]) => start + duration > mark).map(([, duration]) => duration),
                slices: mark,
              });
            }, 300),
          ),
      );
      expect(slices, "the finish mark was set, so the partition is real").not.toBe(Infinity);
      // The budget: the slices aim at 40 ms; one page may run over that alone,
      // and admission (`beginPdfImport`, O(file)) is one task. 200 ms is five
      // slices, far under what the whole import costs in one task.
      const worst = sliced.length ? Math.max(...sliced) : 0;
      console.log(`pdf-import long tasks: ${pages} pages; before finish ${JSON.stringify(sliced)}; finish+layout ${JSON.stringify(finish)}`);
      expect(
        worst,
        `importing ${pages} pages blocked the main thread for ${worst} ms (long tasks before the finish: ${sliced}; finish and layout: ${finish})`,
      ).toBeLessThan(200);
    } finally {
      await rm(scratch, { recursive: true, force: true });
    }
    expect(consoleErrors).toEqual([]);
  });
});
