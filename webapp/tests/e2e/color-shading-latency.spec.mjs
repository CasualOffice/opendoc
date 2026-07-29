// Automates the "color/shading latency" smoke from
// docs/67-EDITOR-UX-GAP-ANALYSIS.md's Shell UX Implementation PR 1 step. This
// is a coarse wall-clock regression guard against a full reflow storm on a
// shading commit (P1G-FOCUS-001 moved these controls to commit on `change`,
// not on every color-picker drag tick) — not the fine-grained frame-budget
// instrumentation tracked separately under the "Edit hot loop" gap.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

// A generous ceiling: catches a gross regression (e.g. a full page re-render
// or repeated stats/outline rebuild on every shading commit), not a tight
// frame budget.
const MAX_SHADE_COMMIT_MS = 500;

test("committing a paragraph shading color stays under a coarse latency budget", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const elapsedMs = await page.evaluate(
    () =>
      new Promise((resolve) => {
        const shade = document.getElementById("paraShade");
        const start = performance.now();
        shade.value = "#ff0000";
        shade.dispatchEvent(new Event("change", { bubbles: true }));
        // Two rAFs give the resulting repaint a chance to actually composite,
        // not just enqueue, before the clock stops.
        requestAnimationFrame(() => requestAnimationFrame(() => resolve(performance.now() - start)));
      }),
  );

  expect(elapsedMs).toBeLessThan(MAX_SHADE_COMMIT_MS);
  expect(consoleErrors).toEqual([]);

  await page.locator("#undoBtn").click();
});
