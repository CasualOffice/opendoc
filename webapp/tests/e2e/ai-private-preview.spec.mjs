// `editor.ai`'s private preview (`docs/178` §9, `docs/181` §4.4): a host's own
// assistant proposes an edit plan to the live editor; the person sees exactly
// what it would do, privately, and nothing reaches the document or the undo
// stack until they Accept. Driven through `window.opendoc.ai`, the in-process
// host contract a host's assistant calls.
import { test, expect, gotoEditor, expectEditorFocused, expectNothingToUndo, MOD } from "./fixtures.mjs";

test.describe.configure({ timeout: 120_000 });

const MARKER = " (assistant)";

/** The first paragraph with text, as the assistant reads it: its id and its first word. */
async function firstParagraph(page) {
  return page.evaluate(async () => {
    const read = await window.opendoc.ai.read({ tool: "read", maxTokens: 4000 });
    if (!read.ok) throw new Error(JSON.stringify(read));
    const block = read.value.blocks.find((b) => b.kind === "paragraph" && /\w{3,}/.test(b.text ?? ""));
    return { node: block.id, word: block.text.match(/\w{3,}/)[0], revision: read.value.revision };
  });
}

/** A plan that inserts MARKER after the paragraph's first word, as a suggestion. */
function planFor({ node, word, revision }) {
  return {
    base_revision: revision,
    edits: [{ op: "insert_text", at: { node, exact: word, occurrence: 1, side: "after" }, text: MARKER }],
  };
}

/** Whether the document holds MARKER, asked of the engine through the assistant's search. */
async function documentHasMarker(page) {
  return page.evaluate(async (marker) => {
    const hits = await window.opendoc.ai.read({ tool: "search", query: marker.trim() });
    return hits.value.hits.length > 0;
  }, MARKER);
}

async function trackedAuthors(page) {
  return page.evaluate(async () => {
    const changes = await window.opendoc.ai.read({ tool: "trackedChanges" });
    return changes.value.trackedChanges.map((change) => change.author);
  });
}

async function preview(page, plan) {
  return page.evaluate((p) => window.opendoc.ai.preview(p), plan);
}

test("a preview is private: the redline shows, the document does not change, Accept lands a tracked AI suggestion that undoes", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await page.evaluate(() => window.opendoc.identify({ id: "ada", name: "Ada Lovelace" }));
  const target = await firstParagraph(page);
  const decisions = [];
  await page.exposeFunction("recordDecision", (detail) => decisions.push(detail));
  await page.evaluate(() => window.opendoc.on("ai-decision", (event) => window.recordDecision(event.detail)));

  const shown = await preview(page, planFor(target));
  expect(shown.ok).toBe(true);
  expect(shown.wouldApply).toBe(true);
  expect(shown.preview.author).toBe("Ada Lovelace (AI)");

  const panel = page.locator("#aiPreviewPanel");
  await expect(panel).toBeVisible();
  await expect(panel).toHaveAttribute("role", "dialog");
  await expect(panel.locator("#aiPreviewTitle")).toHaveText("Assistant suggestion");
  await expect(panel.locator("#aiPreviewPrivate")).toHaveText("Only you can see this until you accept it.");
  await expect(panel.locator("#aiPreviewMode")).toContainText("Ada Lovelace (AI)");
  await expect(panel.locator("ins")).toContainText(MARKER.trim());

  // Private: nothing reached the document or its history.
  expect(await documentHasMarker(page)).toBe(false);
  expect(await trackedAuthors(page)).toEqual([]);
  await expectNothingToUndo(page);

  await panel.getByRole("button", { name: "Accept" }).click();
  await expect(panel).toBeHidden();
  await expect.poll(() => documentHasMarker(page)).toBe(true);
  const authors = await trackedAuthors(page);
  expect(authors.length).toBeGreaterThan(0);
  expect(new Set(authors)).toEqual(new Set(["Ada Lovelace (AI)"]));
  await expect.poll(() => decisions.map((d) => d.decision)).toEqual(["accepted"]);
  expect(decisions[0].previewId).toBe(shown.previewId);

  // One undo step removes the whole plan.
  await page.locator("#pages").focus();
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(() => documentHasMarker(page)).toBe(false);
  expect(await trackedAuthors(page)).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("Discard leaves the document and the undo stack exactly as they were", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  const target = await firstParagraph(page);
  const shown = await preview(page, planFor(target));
  expect(shown.wouldApply).toBe(true);
  const panel = page.locator("#aiPreviewPanel");
  await panel.getByRole("button", { name: "Discard" }).click();
  await expect(panel).toBeHidden();
  expect(await documentHasMarker(page)).toBe(false);
  await expectNothingToUndo(page);
  // Nothing is left to accept: a late Accept is refused, not applied. (That the
  // ENGINE dropped the compiled plan is `ai.rs`'s `discarding_leaves_no_trace`.)
  const late = await page.evaluate(() => window.opendoc.ai.accept());
  expect(late.ok).toBe(false);
  expect(await documentHasMarker(page)).toBe(false);
  expect(consoleErrors).toEqual([]);
});

test("the panel is keyboard operable: focus lands on Accept, Escape discards, Ctrl/Cmd+Enter accepts and returns focus to the page", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  const target = await firstParagraph(page);
  const panel = page.locator("#aiPreviewPanel");
  const accept = panel.getByRole("button", { name: "Accept" });

  await preview(page, planFor(target));
  await expect(accept).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(panel).toBeHidden();
  expect(await documentHasMarker(page)).toBe(false);
  await expectEditorFocused(page);

  await preview(page, planFor(await firstParagraph(page)));
  await expect(accept).toBeFocused();
  // Tab reaches Discard and back: both actions are in the tab order.
  await page.keyboard.press("Shift+Tab");
  await expect(panel.getByRole("button", { name: "Discard" })).toBeFocused();
  await page.keyboard.press(`${MOD}+Enter`);
  await expect(panel).toBeHidden();
  await expect.poll(() => documentHasMarker(page)).toBe(true);
  await expectEditorFocused(page);
  expect(consoleErrors).toEqual([]);
});

test("a readonly mount is refused with a visible reason and nothing to accept", async ({ page, consoleErrors }) => {
  await gotoEditor(page, "&mode=readonly");
  const access = await page.evaluate(() => window.opendoc.ai.access());
  expect(access.access).toBe("read");
  const target = await firstParagraph(page);
  const shown = await preview(page, planFor(target));
  expect(shown.ok).toBe(true);
  expect(shown.wouldApply).toBe(false);
  expect(shown.previewId).toBeNull();
  expect(shown.preview.edits[0].refusal.code).toBe("capability-withheld");

  const panel = page.locator("#aiPreviewPanel");
  await expect(panel).toBeVisible();
  await expect(panel.locator(".ai-preview-refusals")).toContainText(
    "Your access to this document doesn’t allow this change.",
  );
  const accept = panel.getByRole("button", { name: "Accept" });
  await expect(accept).toBeDisabled();
  await expect(panel.locator("#aiPreviewBlocked")).toHaveText("This suggestion can’t be applied.");
  // Focus goes to the one action that can run.
  await expect(panel.getByRole("button", { name: "Discard" })).toBeFocused();
  // And `apply` is refused the same way, by the engine.
  const applied = await page.evaluate((p) => window.opendoc.ai.apply(p), planFor(target));
  expect(applied.ok).toBe(false);
  expect(applied.refusal.code).toBe("capability-withheld");
  expect(await documentHasMarker(page)).toBe(false);
  await page.keyboard.press("Escape");
  await expect(panel).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("a preview mount grants an assistant nothing, not even reads", async ({ page }) => {
  await gotoEditor(page, "&mode=preview");
  const read = await page.evaluate(() => window.opendoc.ai.read({ tool: "outline" }));
  expect(read.ok).toBe(false);
  expect(read.refusal.code).toBe("capability-withheld");
  await expect(page.locator("#aiPreviewPanel")).toHaveCount(0);
});
