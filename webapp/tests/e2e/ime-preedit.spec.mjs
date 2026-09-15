// Automates the "IME live preedit" P0 row from
// docs/67-EDITOR-UX-GAP-ANALYSIS.md: composition text must be visible at the
// caret (compositionupdate) without ever being committed to the document
// until compositionend — the gap this closes is that no compositionupdate
// listener existed at all (P1G-IME-001 only handled start/end).
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

const MOD = process.platform === "darwin" ? "Meta" : "Control";

async function findStatusFor(page, query) {
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(query);
  const status = await page.locator("#findStatus").textContent();
  await page.keyboard.press("Escape");
  return status;
}

test("live preedit shows composing text without committing it, then commits on compositionend", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Dispatch on the FOCUS OWNER, not on `document`. A real IME targets the
  // focused editable element and lets the event bubble; dispatching at
  // `document` is a shape no browser produces, and it is exactly why this spec
  // stayed green for months while the feature was unreachable — the surface
  // that held focus was a non-editable div, which fires no composition events
  // at all (docs/105 UX-001/UX-002, CQ-003). `editable-focus-owner.spec.mjs`
  // guards the precondition; this test now exercises the real path.
  await page.evaluate(() => {
    document.activeElement.dispatchEvent(
      new CompositionEvent("compositionstart", { data: "", bubbles: true, cancelable: true }),
    );
  });
  await page.evaluate(() => {
    document.activeElement.dispatchEvent(
      new CompositionEvent("compositionupdate", {
        data: "PREEDITWORD",
        bubbles: true,
        cancelable: true,
      }),
    );
  });

  await expect(page.locator(".ime-preedit")).toHaveText("PREEDITWORD");
  expect(await findStatusFor(page, "PREEDITWORD")).toBe("No match");

  await page.evaluate(() => {
    document.activeElement.dispatchEvent(
      new CompositionEvent("compositionend", {
        data: "PREEDITWORD",
        bubbles: true,
        cancelable: true,
      }),
    );
  });

  await expect(page.locator(".ime-preedit")).toHaveCount(0);
  expect(await findStatusFor(page, "PREEDITWORD")).toBe("1 match");

  await page.locator("#undoBtn").click();
  expect(consoleErrors).toEqual([]);
});
