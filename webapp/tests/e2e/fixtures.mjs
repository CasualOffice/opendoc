import { test as base, expect } from "@playwright/test";

const MOD = process.platform === "darwin" ? "Meta" : "Control";
const WORD_MOD = process.platform === "darwin" ? "Alt" : "Control";

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

export { expect, MOD, WORD_MOD };

/** A laid-out element's box, retried until it is real.
 *
 *  `boundingBox()` returns null while an element is mid-re-render, so a spec that
 *  polls for a non-zero width and then calls it AGAIN can still get null — under
 *  parallel load that surfaced as `Cannot read properties of null (reading 'x')`,
 *  a flake that reads exactly like a broken canvas. */
export async function stableBox(locator) {
  let box = null;
  await expect
    .poll(async () => {
      box = await locator.boundingBox();
      return box?.width ?? 0;
    })
    .toBeGreaterThan(0);
  return box;
}

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
        document.querySelectorAll(".page-wrap").length > 0 &&
        // The document now paints before the ~9.5 MB of named web fonts arrive,
        // then re-renders once they register. Tests wait for that upgrade so a
        // geometry assertion can never race the repaint; the product does not.
        document.body.dataset.fontsReady === "true"
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

/**
 * Asserts the editing surface holds focus.
 *
 * Use this instead of `expect(page.locator("#pages")).toBeFocused()`. Focus is
 * owned by the editable proxy `#editorTextInput`, not by `#pages` — a
 * non-editable div cannot own text input, so it raises no soft keyboard and
 * fires no composition events (docs/105 UX-001). `#pages` is still focusable
 * and still the skip-link target; focus landing there is immediately handed to
 * the proxy.
 *
 * Asserting either element is correct, because the question these tests are
 * really asking is "can the editor receive text?" — which is why they should
 * not name a specific element at all.
 */
export async function expectEditorFocused(page) {
  const active = await page.evaluate(() => document.activeElement?.id ?? "");
  if (active !== "pages" && active !== "editorTextInput") {
    throw new Error(
      `expected the editing surface to hold focus, but focus is on "${active || "(none)"}"`,
    );
  }
}

// Moves the caret to the very start of the document (⌘/Ctrl+Home), so
// later assertions do not depend on where the initial click happened to land.
export async function moveCaretToDocStart(page) {
  await page.keyboard.press(`${MOD}+Home`);
}

// The three-state review-mode control (`#reviewModeControl`: Editing /
// Suggesting / Viewing) lives in the persistent footer. Selects `mode` and
// confirms its segment became the pressed one.
export async function setReviewMode(page, mode) {
  const button = page.locator(`#reviewModeControl [data-review-mode="${mode}"]`);
  await button.click();
  await expect(button).toHaveAttribute("aria-pressed", "true");
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
