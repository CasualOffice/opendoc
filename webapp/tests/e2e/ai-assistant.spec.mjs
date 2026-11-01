// Casual AI, the first-party assistant plugin (`docs/183` §13), in a real
// browser, against a MOCK provider: a Playwright route on this test server that
// speaks OpenAI Chat Completions with SSE streaming. No real model, no real key.
//
// The owner's rules (2026-10-31) are asserted, not assumed: with no provider the
// editor shows nothing of it; once configured there are exactly three entry
// points (the ✦ item in the selection toolbar, the panel in the side-panel slot,
// palette rows); every change waits in the preview and lands as a suggestion;
// streaming happens in the panel; Stop stops.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

test.describe.configure({ timeout: 120_000 });

const WORD = "CASUALPROBE";
const ASSISTANT = 'iframe[data-plugin="org.casualoffice.assistant"]';

/** Configures BYOK before the page boots: the endpoint is this test server. */
async function withProvider(page) {
  // Init scripts run in every frame; the plugin's sandboxed one has no storage
  // at all (its opaque origin is the point), so only the editor's frame is set.
  await page.addInitScript(() => {
    try {
      localStorage.setItem(
        "opendoc.ai.provider",
        JSON.stringify({ preset: "custom", protocol: "openai", baseUrl: `${location.origin}/mock-ai/v1`, model: "mock-model", apiKey: "sk-test-not-real" }),
      );
    } catch {
      /* the plugin frame */
    }
  });
}

/** The mock model. `answers` maps a phrase of the system prompt to the reply;
 *  `hold` keeps a stream open after its first chunk (for Stop). */
async function mockModel(page, answers, { hold = false } = {}) {
  const seen = [];
  await page.route("**/mock-ai/v1/chat/completions", async (route) => {
    const body = route.request().postDataJSON();
    seen.push(body);
    const system = body.messages[0].content;
    const key = Object.keys(answers).find((k) => system.includes(k)) ?? "default";
    const text = answers[key] ?? "OK";
    if (!body.stream) {
      await route.fulfill({ contentType: "application/json", body: JSON.stringify({ id: "x", object: "chat.completion", created: 1, model: "mock-model", choices: [{ index: 0, message: { role: "assistant", content: text }, finish_reason: "stop" }] }) });
      return;
    }
    const pieces = text.match(/.{1,6}/gs) ?? [text];
    const chunks = pieces.map((piece) => `data: ${JSON.stringify({ id: "x", model: "mock-model", choices: [{ index: 0, delta: { content: piece }, finish_reason: null }] })}\n\n`);
    if (hold) {
      // The model never answers: only the Stop button can end the wait.
      await new Promise((resolve) => setTimeout(resolve, 60_000));
      await route.fulfill({ contentType: "text/event-stream", body: chunks.join("") }).catch(() => {});
      return;
    }
    chunks.push(`data: ${JSON.stringify({ id: "x", model: "mock-model", choices: [{ index: 0, delta: {}, finish_reason: "stop" }] })}\n\n`, "data: [DONE]\n\n");
    await route.fulfill({ contentType: "text/event-stream", body: chunks.join("") });
  });
  return seen;
}

/** Types WORD on a new paragraph and selects it. */
async function selectWord(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type(WORD);
  for (let i = 0; i < WORD.length; i += 1) await page.keyboard.press("Shift+ArrowLeft");
  await expect(page.locator("#selToolbar")).toBeVisible();
}

const assistant = (page) => page.frameLocator(ASSISTANT);

async function waitForAssistant(page) {
  await expect(page.locator(ASSISTANT)).toHaveCount(1, { timeout: 30_000 });
  await expect(assistant(page).locator("html")).toHaveAttribute("data-ready", "1", { timeout: 30_000 });
}

test("with no AI provider the editor shows nothing of Casual AI and runs none of it", async ({ page }) => {
  await gotoEditor(page);
  await selectWord(page);
  await expect(page.locator("#selAiBtn")).toBeHidden();
  await expect(page.locator("#selAiDivider")).toBeHidden();
  await expect(page.locator("#reviewAddinsBtn")).toBeHidden();
  await expect(page.locator("#pluginPanel")).toBeHidden();
  await expect(page.locator("iframe[data-plugin]")).toHaveCount(0);
  // No palette row: the AI rows exist only while the assistant runs.
  const rows = await page.evaluate(() => window.opendoc.describe().commands.map((c) => c.id).filter((id) => id.startsWith("plugin.") && id !== "plugin.manage"));
  expect(rows).toEqual([]);
  // ⌘/ does nothing visible.
  await page.keyboard.press(`${MOD}+/`);
  await expect(page.locator("#selAiMenu")).toBeHidden();
  await expect(page.locator("#pluginPanel")).toBeHidden();
});

test("✦ Rewrite works at the selection, streams in live, and lands as a suggestion by Casual AI", async ({ page }) => {
  await withProvider(page);
  const seen = await mockModel(page, { "Reply with the replacement text only": "CASUALREWRITTEN" });
  await gotoEditor(page);
  await waitForAssistant(page);
  await selectWord(page);
  await expect(page.locator("#selAiBtn")).toBeVisible();
  await page.locator("#selAiBtn").click();
  const menu = page.locator("#selAiMenu");
  await expect(menu).toBeVisible();
  await expect(menu.locator(".menu-item > span:last-child")).toHaveText(["Rewrite", "Shorten", "Expand", "Fix grammar", "Make more formal", "Make friendlier", "Make more confident", "Translate…", "Turn into table", "Ask Casual AI"]);
  await menu.locator(".menu-item", { hasText: "Rewrite" }).click();
  // As Google Docs' Refine and Notion's Edit with AI: the action works where the
  // selection is. The side panel stays closed; the rewrite streams in live, and
  // the editor's turn bar holds Stop, Accept all and Reject all (`docs/183` §23).
  await expect(page.locator("#aiTurnBar")).toBeVisible();
  await expect(page.locator("#pluginPanel")).toBeHidden();
  await expect(page.locator("#aiPreviewPanel")).toHaveCount(0);
  const has = (word) => page.evaluate(async (w) => (await window.opendoc.ai.read({ tool: "search", query: w })).value.hits.length, word);
  await expect.poll(() => has("CASUALREWRITTEN")).toBeGreaterThan(0);
  // The selected text went to the model data-marked, never as an instruction.
  expect(seen[0].messages.at(-1).content).toContain(`<document_text id="selection">${WORD}</document_text>`);
  expect(seen[0].stream).toBe(true);
  // The exchange is still in the panel's history for whoever opens it.
  await expect(assistant(page).locator(".ca-msg-ai").first().locator(".ca-diff ins")).toContainText("CASUALREWRITTEN");
  const authors = await page.evaluate(async () => (await window.opendoc.ai.read({ tool: "trackedChanges" })).value.trackedChanges.map((c) => c.author));
  expect(authors.length).toBeGreaterThan(0);
  expect(authors.every((a) => /AI/.test(a ?? ""))).toBe(true);
});

test("Stop ends a stream at once and says so", async ({ page }) => {
  await withProvider(page);
  await mockModel(page, { "answering questions": "The budget is" }, { hold: true });
  await gotoEditor(page);
  await waitForAssistant(page);
  await page.evaluate(() => window.opendoc.execute("plugin.org.casualoffice.assistant.ask"));
  // Ask opens the panel in the side-panel slot, titled by the product's catalogue.
  await expect(page.locator("#pluginPanel")).toBeVisible();
  await expect(page.locator("#pluginPanel")).toHaveClass(/side-panel/);
  await expect(page.locator("#pluginPanelTitle")).toHaveText("Casual AI");
  const frame = assistant(page);
  await frame.locator("#prompt").fill("What is the budget?");
  await frame.locator("#prompt").press("Enter");
  await expect(frame.locator("#stop")).toBeVisible();
  await frame.locator("#stop").click();
  await expect(frame.locator("#stop")).toBeHidden();
  await expect(frame.locator("#send")).toBeVisible();
  await expect(frame.locator(".ca-msg-ai").last()).toContainText("Stopped.");
});

test("Ask answers with citation chips that jump to the paragraph, and history survives a reload until cleared", async ({ page }) => {
  await withProvider(page);
  await mockModel(page, { "answering questions": "It is described here [p1]." });
  await gotoEditor(page);
  await waitForAssistant(page);
  await page.evaluate(() => window.opendoc.execute("plugin.org.casualoffice.assistant.ask"));
  const frame = assistant(page);
  await frame.locator("#prompt").fill("Where is the title?");
  await frame.locator("#send").click();
  const chip = frame.locator(".ca-cite").first();
  await expect(chip).toHaveAttribute("aria-label", "Go to paragraph 1");
  // Every answer is a card: a primary action that changes the document, a
  // second one, Copy and Try again (Gemini's side panel); one line under the
  // composer says the model can be wrong.
  const actions = frame.locator(".ca-card").last().locator(".ca-card-actions button");
  await expect(actions).toHaveCount(4);
  await expect(actions.nth(0)).toHaveText("Insert at the cursor");
  await expect(actions.nth(0)).toHaveClass(/btn-primary/);
  await expect(actions.nth(1)).toHaveText("Add as comment");
  await expect(actions.nth(2)).toHaveAttribute("aria-label", "Copy");
  await expect(actions.nth(3)).toHaveAttribute("aria-label", "Try again");
  await expect(frame.locator("#caveat")).toContainText("AI can make mistakes");
  await chip.click();
  // Focus moved to the document, at the cited paragraph.
  await expect.poll(() => page.evaluate(() => document.activeElement?.id ?? "")).toMatch(/^(pages|editorTextInput)$/);

  await page.reload();
  await waitForAssistant(page);
  await page.evaluate(() => window.opendoc.execute("plugin.org.casualoffice.assistant.ask"));
  await expect(assistant(page).locator(".ca-msg-user")).toContainText(["Where is the title?"]);
  // Clear history is behind the panel's ⋯, in the one header line.
  await page.locator("#pluginPanelMore").click();
  await page.locator("#pluginPanelMenu .menu-item", { hasText: "Clear history" }).click();
  await expect(assistant(page).locator(".ca-msg-user")).toHaveCount(0);
});

test("the review pass applies comments and suggestions in one live turn", async ({ page }) => {
  await withProvider(page);
  await gotoEditor(page);
  // The answer quotes the document's real first paragraph, read the way the
  // assistant reads it, so the verifier keeps it.
  const first = await page.evaluate(async () => {
    const read = await window.opendoc.ai.read({ tool: "read", maxTokens: 4000 });
    return read.value.blocks.find((b) => b.kind === "paragraph" && /\w{4,}/.test(b.text)).text.match(/\w{4,}/)[0];
  });
  await mockModel(page, {
    "reviewing a document": JSON.stringify([
      { id: "p1", quote: first, comment: "Clarify this.", replacement: first.toUpperCase() },
      { id: "p99", quote: "nothing", comment: "invented" },
    ]),
  });
  await waitForAssistant(page);
  await page.evaluate(() => window.opendoc.execute("plugin.org.casualoffice.assistant.review"));
  // The findings show in the card first; Apply as suggestions writes them, live.
  const card = assistant(page).locator(".ca-card").last();
  await expect(card).toContainText("Comments: 1. Suggestions: 1.");
  await card.locator('[data-action="apply"]').click();
  await expect(page.locator("#aiTurnBar")).toHaveAttribute("data-state", "done", { timeout: 30_000 });
  const read = (tool) => page.evaluate(async (t) => (await window.opendoc.ai.read({ tool: t })).value, tool);
  expect((await read("comments")).comments.some((c) => c.text === "Clarify this." && c.author === "Casual AI")).toBe(true);
  expect((await read("trackedChanges")).trackedChanges.some((c) => c.author === "Casual AI")).toBe(true);
  await expect(page.locator("#aiPreviewPanel")).toHaveCount(0);
});

test("⌘/ opens the ✦ actions on a selection", async ({ page }) => {
  await withProvider(page);
  await mockModel(page, {});
  await gotoEditor(page);
  await waitForAssistant(page);
  await selectWord(page);
  await page.keyboard.press(`${MOD}+/`);
  await expect(page.locator("#selAiMenu")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator("#selAiMenu")).toBeHidden();
});
