// One guard, parameterised over the operations that touch the whole document.
//
// `docs/116`: the tab freezes when something the main thread does costs more
// the bigger the document is. Three of those were found one owner report at a
// time — opening a file, opening the Outline panel, and an ordinary click — so
// the guard is written over the LIST rather than over any one of them. A new
// panel or command that enumerates the document belongs in `OPERATIONS`, and
// that is the point: adding it is how the next one is caught here instead of in
// the owner's afternoon.
//
// What is asserted is the GUARANTEE, not the mechanism: no single main-thread
// task during the operation exceeds its budget, so the frame loop keeps
// running, a progress indicator can paint and a click can land. "The operation
// finished" is exactly the assertion that let this ship — the Outline panel
// finished too, eventually, after an estimated 1.6 hours.
//
// The document is 40,000 plain paragraphs: small enough to open in under a
// second, large enough that a per-node document scan is unmissable. Budgets are
// set several times over what the fixed code measures and far under what the
// quadratic shapes measured, so a loaded machine does not fail the run while a
// reintroduced O(n²) cannot pass it.
//
// The complexity half of the same guard lives in Rust
// (`document_wide_reads_do_not_scan_the_document_once_per_node`), because a
// wall clock cannot tell a quadratic shape from a slow machine. The two
// together are the contract: constant scans, bounded tasks.
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test, expect, gotoEditor } from "./fixtures.mjs";

const LINE = "examplefile.com - Sample Files\r\n"; // 32 bytes, the owner's own line
const BLOCKS = 40_000;

/** Longest main-thread task, in ms, observed while `action` ran. */
async function longestTask(page, action) {
  await page.evaluate(() => {
    window.__tasks = [];
    window.__taskObserver?.disconnect();
    window.__taskObserver = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.__tasks.push(Math.round(entry.duration));
    });
    window.__taskObserver.observe({ entryTypes: ["longtask"] });
  });
  await action();
  // Long-task entries are delivered on a later turn of the event loop, so give
  // the observer a turn before reading: a zero read here would be the guard
  // measuring nothing and passing.
  const tasks = await page.evaluate(
    () => new Promise((resolve) => setTimeout(() => resolve(window.__tasks ?? []), 300)),
  );
  return { worst: tasks.length ? Math.max(...tasks) : 0, tasks };
}

/**
 * Each row is an operation whose cost could scale with the document, its budget
 * for a single main-thread task, and what that budget is derived from.
 */
const OPERATIONS = [
  {
    name: "a click into the page",
    // `objectAt` alone was 1,437 ms of a 1,399 ms click at 20,000 blocks before
    // `docs/116`; it is now 0.7 ms and this measures no long task at all.
    budget: 500,
    async run(page) {
      await page.locator(".page-wrap .page").first().click({ position: { x: 60, y: 70 } });
    },
  },
  {
    name: "moving the pointer across the page",
    // The hover router (`docs/109` HF-179) asks the engine what is under the
    // pointer once per animation frame — `bandAt`, then `objectAt`, then a
    // caret hit test and a form-checkbox lookup, then `linkAt`. Each is bounded
    // (`objectAt` is 0.7 ms at this size since `docs/116`) but they are asked
    // far more often than anything else in this list, so this row is the one
    // that notices if any of them ever becomes a document scan again. A hover
    // must never be the thing that stalls the frame loop.
    budget: 300,
    async run(page) {
      const box = await page.locator(".page-wrap").first().boundingBox();
      for (let i = 0; i < 40; i += 1) {
        await page.mouse.move(box.x + 40 + i * 12, box.y + 40 + i * 9);
      }
      // …and it has to have actually ROUTED. Without this the row would measure
      // a sweep over a surface whose router had silently stopped answering on a
      // large document, and 0 ms is under every budget.
      await expect(page.locator(".page-wrap .page[data-pointer-target]")).toHaveCount(1);
    },
  },
  {
    name: "opening the Outline panel",
    // `documentOutline` was 5,565 ms at this exact size, and ~1.6 hours of
    // arithmetic on the owner's file. It is now ~1 ms.
    budget: 800,
    async run(page) {
      await page.locator("#railOutline").click();
      await expect(page.locator("#outlinePanel")).toBeVisible();
    },
  },
  {
    name: "opening the Pages panel",
    // Already windowed to 40 thumbnails; this pins that it stays windowed.
    budget: 800,
    async run(page) {
      await page.locator("#railPages").click();
      await expect(page.locator("#pagesPanel")).toBeVisible();
    },
  },
];

test.describe("main-thread budget on a large document", () => {
  test.setTimeout(180_000);

  test("no operation blocks the main thread past its budget", async ({ page, consoleErrors }) => {
    await gotoEditor(page);
    const scratch = mkdtempSync(join(tmpdir(), "opendoc-budget-"));
    const file = join(scratch, "budget.txt");
    writeFileSync(file, LINE.repeat(BLOCKS - 1));

    try {
      // Opening is itself an operation, and it is the one still owed a fix
      // (`docs/116` §7: 88% of it is one uninterruptible measure pass). This
      // budget is therefore a RATCHET on the current state — 419 ms measured —
      // not a claim that opening is interactive. It must come down when the
      // measure pass goes lazy, never up.
      const opened = await longestTask(page, async () => {
        await page.locator("#file").setInputFiles(file);
        await page.waitForFunction(
          (want) =>
            Number(
              (document.getElementById("statParas")?.textContent ?? "").replace(/[^0-9]/g, ""),
            ) === want,
          BLOCKS,
          { timeout: 120_000 },
        );
      });
      expect(
        opened.worst,
        `opening ${BLOCKS} blocks blocked the main thread for ${opened.worst} ms (tasks: ${opened.tasks})`,
      ).toBeLessThan(4_000);

      for (const operation of OPERATIONS) {
        const { worst, tasks } = await longestTask(page, () => operation.run(page));
        expect(
          worst,
          `${operation.name} blocked the main thread for ${worst} ms against a ${operation.budget} ms budget (tasks: ${tasks})`,
        ).toBeLessThan(operation.budget);
      }
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }

    expect(consoleErrors).toEqual([]);
  });
});
