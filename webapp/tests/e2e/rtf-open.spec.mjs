// A user with a .rtf can open it. The adapter is proven in Rust
// (`crates/casual-doc-rtf`); this asserts the thing the adapter cannot: that
// the PRODUCT reaches it. The picker's `accept` list, `handleFile`'s extension
// pre-filter and `format_io.mjs`'s catalog are three independent places that
// each silently drop an unlisted extension, and an importer none of them names
// is "built but not reachable" — docs/105 §9 rule 4, the most expensive
// recurring defect class in this repository.
import { test, expect, gotoEditor } from "./fixtures.mjs";

const RTF = "tests/e2e/sample.rtf";

test("a .rtf opens through the real file path, with its formatting", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // Record every value the status strip takes: the open path names the file
  // while it works and then clears the strip, so polling for one edge can miss
  // it entirely on a fast open (the pattern insert-surface.spec.mjs uses).
  await page.evaluate(() => {
    window.__statusLog = [];
    const el = document.getElementById("status");
    new MutationObserver(() => window.__statusLog.push(el.textContent)).observe(el, {
      childList: true,
      characterData: true,
      subtree: true,
    });
  });

  await page.locator("#file").setInputFiles(RTF);

  // It opened: pages painted, and the strip never reported a refusal.
  await expect.poll(() => page.locator(".page-wrap").count()).toBeGreaterThan(0);
  const log = await page.evaluate(() => window.__statusLog);
  expect(log.join(" | ")).not.toMatch(/Please choose|error|cannot|unsupported/i);

  // The TEXT arrived — not an empty document that merely failed quietly.
  // Read it from the accessibility mirror: the pages are painted to canvas, so
  // #pages carries only the line-number gutter, and asserting over that would
  // have passed on a blank document.
  const text = await page.locator("#a11yDocument").evaluate((el) => el.textContent);
  expect(text).toContain("Quarterly Review");
  expect(text).toContain("Closing line.");
  // `舦 ?` — the unicode escape wins over its ASCII fallback.
  expect(text).toContain("•");
  expect(text).not.toContain("�");

  // The table survived as a table, not as two stray paragraphs.
  expect(text).toContain("Left cell");
  expect(text).toContain("Right cell");

  expect(consoleErrors).toEqual([]);
});
