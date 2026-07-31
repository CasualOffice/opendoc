// REVIEW-GAP-008 (docs/81-COMMENTS-SUGGESTIONS-COMPLETENESS-AUDIT.md): rich
// paste in Suggesting mode used to flatten every run to one plain-text
// tracked insertion, discarding bold/italic/color/etc. entirely, while
// multi-paragraph paste was rejected outright. Pasting rich content at a
// collapsed caret, single paragraph only, now chains one tracked
// `suggestStyledInsert` per clipboard run under one gesture, so the pasted
// text keeps its per-run formatting as one review card and one Undo step.
// Multi-paragraph paste, and a rich paste that also replaces an existing
// selection, remain explicitly out of scope (see the `pasteTrackedRichRuns`
// doc comment in webapp/src/main.js) and stay on the flattened/plain path.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

async function dispatchClipboardEvent(page, type, data = {}) {
  return page.evaluate(
    ({ type, data }) => {
      const dt = new DataTransfer();
      for (const [mime, value] of Object.entries(data)) dt.setData(mime, value);
      const event = new ClipboardEvent(type, {
        clipboardData: dt,
        bubbles: true,
        cancelable: true,
      });
      document.dispatchEvent(event);
      return { html: dt.getData("text/html"), text: dt.getData("text/plain") };
    },
    { type, data },
  );
}

// `#reviewInlineMode` lives in the Home ribbon panel, hidden while a
// contextual panel is showing — switch tabs first, matching
// suggesting-mode-gate.spec.mjs's helper.
async function enterSuggestingMode(page) {
  await page.locator("#tabHome").click();
  await page.locator("#reviewInlineMode").click();
  await expect(page.locator("#reviewInlineMode")).toHaveText("Suggesting");
}

test("rich single-paragraph paste in Suggesting mode preserves per-run formatting as one tracked suggestion", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await enterSuggestingMode(page);
  await moveCaretToDocStart(page);

  await dispatchClipboardEvent(page, "paste", {
    "text/html": "<p>Plain <b>Bold</b></p>",
    "text/plain": "Plain Bold",
  });

  // One review card for the whole paste, not one per run — proves the
  // per-run suggestStyledInsert calls coalesced into a single tracked group.
  const sidebar = page.locator("#reviewSidebar");
  const cards = sidebar.locator(".review-margin-card.review-margin-insertion");
  await expect(cards).toHaveCount(1);
  await expect(cards).toContainText("Plain Bold");

  // The pasted text itself carries distinct per-run formatting, not one
  // uniform (flattened) format: select the whole pasted range and copy it
  // back out through `copyRichRuns` (the same ground-truth run inspection
  // `clipboard-rich.spec.mjs` uses) — the internal marker must show "Bold"
  // alone as bold and "Plain " as unformatted, not one uniform run.
  //
  // (Not asserted through the `#bold` toolbar button: reflecting format at a
  // selection inside a pending tracked revision is a separate, pre-existing
  // gap in the toolbar's own reflection, not something this paste fix
  // introduces or is responsible for — `copyRichRuns` is the actual model
  // data this fix is about.)
  await moveCaretToDocStart(page);
  for (let i = 0; i < "Plain Bold".length; i++) await page.keyboard.press("Shift+ArrowRight");
  const clip = await dispatchClipboardEvent(page, "copy");
  expect(clip.text).toBe("Plain Bold");
  expect(clip.html).toMatch(/^<!--opendoc-clipboard-runs:/);
  expect(clip.html).toContain("Plain ");
  expect(clip.html).toMatch(/<b>Bold<\/b>/);
  expect(clip.html).not.toMatch(/<b>Plain/);

  // One Undo removes the entire paste (one atomic action), not just the
  // last run.
  await moveCaretToDocStart(page);
  await page.locator("#undoBtn").click();
  await expect(cards).toHaveCount(0);
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("Plain Bold");
  await expect(page.locator("#findStatus")).toHaveText("No match");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("multi-paragraph rich paste in Suggesting mode is still rejected, not silently flattened", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await enterSuggestingMode(page);
  await moveCaretToDocStart(page);

  await dispatchClipboardEvent(page, "paste", {
    "text/html": "<p>First paragraph</p><p>Second paragraph</p>",
    "text/plain": "First paragraph\nSecond paragraph",
  });

  await expect(page.locator("#status")).toContainText("cannot be tracked");
  const sidebar = page.locator("#reviewSidebar");
  await expect(sidebar.locator(".review-margin-card.review-margin-insertion")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("rich paste that replaces an existing selection in Suggesting mode still tracks a plain-text replacement", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  // Type the target text in ordinary Editing mode first, so it is plain
  // committed content (not itself a pending suggestion) by the time it is
  // selected and pasted over — matching how existing suggestReplace
  // coverage (review-margin.spec.mjs) sets up its replacement fixture.
  await page.keyboard.type("OLDWORD");
  await enterSuggestingMode(page);
  for (let i = 0; i < "OLDWORD".length; i++) await page.keyboard.press("Shift+ArrowLeft");

  await dispatchClipboardEvent(page, "paste", {
    "text/html": "<p>Plain <b>Bold</b></p>",
    "text/plain": "Plain Bold",
  });

  // Replacing a selection has no tracked multi-run *replacement* group yet
  // (REVIEW-GAP-008's remaining scope), so this still lands as one
  // flattened, single-format tracked replacement rather than erroring or
  // silently applying untracked.
  const sidebar = page.locator("#reviewSidebar");
  const replacement = sidebar.locator(".review-margin-card.review-margin-replacement");
  await expect(replacement).toHaveCount(1);
  await expect(replacement).toContainText("Plain Bold");

  expect(consoleErrors).toEqual([]);
});
