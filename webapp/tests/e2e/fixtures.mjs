import { test as base, expect } from "@playwright/test";

const MOD = process.platform === "darwin" ? "Meta" : "Control";

// Collects console errors/pageerrors for the duration of a test so specs can
// assert none occurred instead of only checking the behavior they triggered.
export const test = base.extend({
  consoleErrors: async ({ page }, use) => {
    const errors = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });
    page.on("pageerror", (err) => errors.push(String(err)));
    await use(errors);
  },
});

export { expect, MOD };

// Navigates to the demo document and waits for the WASM engine to boot, the
// sample to open, and every page to finish its first render (mirrors the
// manual "headless browser smoke" checks previously narrated in
// docs/14-EXECUTION-TRACKER.md — see docs/67-EDITOR-UX-GAP-ANALYSIS.md).
export async function gotoEditor(page) {
  await page.goto("/editor.html?fixture=rich");
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status !== null &&
        status.textContent === "" &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length > 0
      );
    },
    null,
    { timeout: 45_000 },
  );
}

// Clicks into the first rendered page to focus the editor surface and give
// the engine an initial hit-tested caret, independent of the demo's exact
// text layout.
export async function clickIntoFirstPage(page) {
  await page.locator(".page-wrap .page").first().click({ position: { x: 60, y: 60 } });
}

// Moves the caret to the very start of the document (⌘/Ctrl+Home), so
// later assertions do not depend on where the initial click happened to land.
export async function moveCaretToDocStart(page) {
  await page.keyboard.press(`${MOD}+Home`);
}

// Types `marker` at the caret, rewinds to just before it, then proves the
// editor is still live by finding it via the real Find panel — the same
// "click, type, find" recovery check used for every focus-recovery scenario.
// Ends by undoing the insertion so specs stay independent of each other.
export async function typeMoveFindUndo(page, marker) {
  await page.keyboard.type(marker);
  await moveCaretToDocStart(page);
  await page.keyboard.press(`${MOD}+f`);
  const findInput = page.locator("#findInput");
  await findInput.fill(marker);
  await expect(page.locator("#findStatus")).toHaveText("1 match");
  await page.keyboard.press("Escape");
  await page.locator("#undoBtn").click();
}
