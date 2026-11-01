// Casual AI must never lose the person's text, and must read the whole document
// (`docs/183` §23.6), against a mock provider in the page (no real model).
//
// The owner's report (2026-11-01, Ollama llama3.2:3b): "Make more confident"
// left only `Deleted: <the sentence>` and then "Accepted."; a multi-paragraph
// "Make friendlier" and a "Translate" failed; and "it can't access the full doc,
// or tables". Each guarantee is asserted on the DOCUMENT, read the way the
// assistant reads it, or on the request the model received — never on a message.
//
//   * an EMPTY answer, a STALLED stream that is stopped, and a stream whose
//     connection DROPS leave the document byte for byte as it was;
//   * a rewrite across paragraphs replaces only the selected part of each
//     paragraph, one plain-text request per paragraph, and leaves a table alone;
//   * a question reaches the model with the table's cells, the header, the footer
//     and the footnote, each marked with where it comes from.
import { fileURLToPath } from "node:url";
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

test.describe.configure({ timeout: 150_000 });

const ASSISTANT = 'iframe[data-plugin="org.casualoffice.assistant"]';
const WORD = "CASUALSAFETYPROBE in a sentence the AI must not lose.";
const corpus = (name) => fileURLToPath(new URL(`../../../fixtures/corpus/${name}`, import.meta.url));

/** A provider pointed at a mock endpoint served from INSIDE the page. A rule is
 *  `{match, text, mode}`: `mode` "hold" never sends a byte until aborted, "drop"
 *  fails the connection after `delay` ms with nothing sent. */
async function mockProvider(page, { editAs = "direct" } = {}) {
  await page.addInitScript((mode) => {
    try {
      localStorage.setItem(
        "opendoc.ai.provider",
        JSON.stringify({ preset: "custom", protocol: "openai", baseUrl: `${location.origin}/mock-ai/v1`, model: "mock-model", apiKey: "sk-test-not-real" }),
      );
      localStorage.setItem("opendoc.ai.editAs", mode);
    } catch {
      /* the plugin frame has no storage */
    }
    const real = window.fetch.bind(window);
    const mock = { rules: [], fallback: "OK", delay: 10, seen: [] };
    window.__mockAi = mock;
    window.fetch = async (input, init = {}) => {
      const url = typeof input === "string" ? input : input.url;
      if (!String(url).includes("/mock-ai/v1/chat/completions")) return real(input, init);
      const body = JSON.parse(init.body);
      mock.seen.push(body);
      const all = body.messages.map((m) => m.content).join("\n");
      const rule = mock.rules.find((r) => new RegExp(r.match).test(all)) ?? { text: mock.fallback };
      const signal = init.signal;
      const encoder = new TextEncoder();
      const chunk = (content, finish = null) => `data: ${JSON.stringify({ id: "x", model: "mock-model", choices: [{ index: 0, delta: content === null ? {} : { content }, finish_reason: finish }] })}\n\n`;
      const stream = new ReadableStream({
        async start(controller) {
          if (rule.mode === "hold") {
            await new Promise((resolve) => signal?.addEventListener("abort", resolve, { once: true }));
            controller.error(new DOMException("Aborted", "AbortError"));
            return;
          }
          if (rule.mode === "drop") {
            await new Promise((resolve) => setTimeout(resolve, mock.delay));
            controller.error(new TypeError("network connection lost"));
            return;
          }
          for (const piece of rule.text.match(/[\s\S]{1,6}/g) ?? []) {
            await new Promise((resolve) => setTimeout(resolve, mock.delay));
            if (signal?.aborted) {
              controller.error(new DOMException("Aborted", "AbortError"));
              return;
            }
            controller.enqueue(encoder.encode(chunk(piece)));
          }
          controller.enqueue(encoder.encode(`${chunk(null, "stop")}data: [DONE]\n\n`));
          controller.close();
        },
      });
      return new Response(stream, { headers: { "content-type": "text/event-stream" } });
    };
  }, editAs);
}

const rules = (page, list) => page.evaluate((list) => {
  window.__mockAi.rules = list;
  window.__mockAi.seen.length = 0;
}, list);
const seen = (page) => page.evaluate(() => window.__mockAi.seen);
const assistant = (page) => page.frameLocator(ASSISTANT);

async function waitForAssistant(page) {
  await expect(page.locator(ASSISTANT)).toHaveCount(1, { timeout: 30_000 });
  await expect(assistant(page).locator("html")).toHaveAttribute("data-ready", "1", { timeout: 30_000 });
}

const blocks = (page) => page.evaluate(async () => (await window.opendoc.ai.read({ tool: "read", maxTokens: 8000 })).value.blocks);
/** The body's text, byte for byte, with every table's cells. */
const snapshot = (page) =>
  page.evaluate(async () => {
    const read = (await window.opendoc.ai.read({ tool: "read", maxTokens: 8000 })).value.blocks;
    const out = [];
    for (const b of read) {
      if (b.kind === "table") {
        const t = (await window.opendoc.ai.read({ tool: "table", table: b.id })).value;
        out.push(`[table ${t.rows.map((r) => r.cells.map((c) => c.text).join("|")).join("/")}]`);
      } else out.push(b.text ?? `[${b.kind}]`);
    }
    return out.join("\n");
  });
const trackedCount = (page) => page.evaluate(async () => (await window.opendoc.ai.read({ tool: "trackedChanges", limit: 200 })).value.trackedChanges.length);

/** A new last paragraph holding WORD, selected. */
async function selectProbe(page) {
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
  await page.keyboard.insertText(WORD);
  await page.keyboard.press("Shift+Home");
  await expect.poll(() => page.evaluate(async () => (await window.opendoc.ai.read({ tool: "search", query: "CASUALSAFETYPROBE" })).value.hits.length)).toBe(1);
}

const run = (page, command) => page.evaluate((id) => window.opendoc.execute(`plugin.org.casualoffice.assistant.${id}`), command);
const liveState = (page) => assistant(page).locator(".ca-live").last();

// ---- An edit turn that writes nothing changes nothing ----------------------------

test("an EMPTY answer leaves the selection byte for byte as it was, and nothing waits for Accept", async ({ page }) => {
  await mockProvider(page);
  await gotoEditor(page);
  await waitForAssistant(page);
  await rules(page, [{ match: "more confident", text: "" }]);
  await selectProbe(page);
  const before = await snapshot(page);
  await run(page, "confident");
  await expect(liveState(page)).toHaveAttribute("data-state", "rolledBack", { timeout: 30_000 });
  expect((await seen(page)).length).toBe(1);
  expect(await snapshot(page)).toBe(before);
  expect(await trackedCount(page)).toBe(0);
  await expect(page.locator("#aiTurnBar")).toBeHidden();
  await expect(liveState(page).locator('[data-live="accept"]')).toHaveCount(0);
});

test("a STALLED stream that is stopped leaves the selection byte for byte as it was", async ({ page }) => {
  await mockProvider(page);
  await gotoEditor(page);
  await waitForAssistant(page);
  await rules(page, [{ match: "more confident", text: "", mode: "hold" }]);
  await selectProbe(page);
  const before = await snapshot(page);
  await run(page, "confident");
  await expect(page.locator("#aiTurnBar")).toBeVisible();
  // Long past the moment the old turn struck the selection (450 ms after it opened).
  await page.waitForTimeout(1200);
  expect(await snapshot(page)).toBe(before);
  await page.locator("#aiTurnBar .ai-turn-stop").click();
  await expect(liveState(page)).toHaveAttribute("data-state", /rolledBack|stopped/, { timeout: 30_000 });
  await expect(page.locator("#aiTurnBar")).toBeHidden();
  expect(await snapshot(page)).toBe(before);
  expect(await trackedCount(page)).toBe(0);
});

test("a stream whose connection DROPS before a word leaves the selection as it was, and says the answer was cut off", async ({ page }) => {
  await mockProvider(page);
  await gotoEditor(page);
  await waitForAssistant(page);
  await rules(page, [{ match: "more confident", text: "", mode: "drop" }]);
  await page.evaluate(() => { window.__mockAi.delay = 900; });
  await selectProbe(page);
  const before = await snapshot(page);
  await run(page, "confident");
  await expect(liveState(page)).toHaveAttribute("data-state", "rolledBack", { timeout: 30_000 });
  expect(await snapshot(page)).toBe(before);
  expect(await trackedCount(page)).toBe(0);
});

// ---- Across paragraphs: per paragraph, the selected part only, tables alone -------

test("a rewrite across paragraphs replaces only the selected part of each, one plain-text request per paragraph", async ({ page }) => {
  await mockProvider(page);
  await gotoEditor(page);
  await waitForAssistant(page);
  await rules(page, [{ match: "warmer, friendlier", text: "ZZ" }]);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  for (const line of ["Alpha one.", "Beta two.", "Gamma three."]) {
    await page.keyboard.press("Enter");
    await page.keyboard.insertText(line);
  }
  // A BACKWARD selection from inside "Gamma three." up into "Alpha one.".
  for (let i = 0; i < 7; i += 1) await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Shift+ArrowUp");
  await page.keyboard.press("Shift+ArrowUp");
  await run(page, "friendly");
  await expect(liveState(page)).toHaveAttribute("data-state", "done", { timeout: 30_000 });
  const requests = await seen(page);
  expect(requests.length).toBe(3);
  for (const body of requests) {
    const user = body.messages.at(-1).content;
    // One paragraph's part per request, as text: no JSON plan to get wrong.
    expect(user).toMatch(/<document_text id="selection">[^\n<]+<\/document_text>$/);
    expect(body.messages[0].content).not.toMatch(/JSON/);
  }
  const text = (await blocks(page)).map((b) => b.text ?? "");
  const alpha = text.find((t) => t.startsWith("Al"));
  const gamma = text.find((t) => t.endsWith("three.") && t !== "Gamma three.");
  expect(text).toContain("ZZ");
  expect(alpha, "the head of the first paragraph, outside the selection, is kept").toMatch(/^Al.*ZZ$/);
  expect(gamma, "the tail of the last paragraph, outside the selection, is kept").toMatch(/^ZZ.*three\.$/);
});

test("Make friendlier over the whole document leaves the table's cells exactly as they were", async ({ page }) => {
  await mockProvider(page);
  await gotoEditor(page);
  await waitForAssistant(page);
  await rules(page, [{ match: "warmer, friendlier", text: "ZZ" }]);
  const tables = (await blocks(page)).filter((b) => b.kind === "table");
  expect(tables.length, "the rich fixture holds a table").toBeGreaterThan(0);
  const cells = () => page.evaluate(async (id) => (await window.opendoc.ai.read({ tool: "table", table: id })).value.rows.map((r) => r.cells.map((c) => c.text)), tables[0].id);
  const before = await cells();
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+a`);
  await run(page, "friendly");
  await expect(liveState(page)).toHaveAttribute("data-state", "done", { timeout: 60_000 });
  expect(await cells()).toEqual(before);
  // No request carried a cell's text: a table is not rewritten by a selection action.
  const flat = before.flat().filter((t) => t.trim().length > 3);
  for (const body of await seen(page)) for (const cell of flat) expect(body.messages.at(-1).content).not.toContain(`>${cell}<`);
  await expect(assistant(page).locator(".ca-card-note").last()).not.toBeEmpty();
});

// ---- The whole document reaches the model -----------------------------------------

async function openCorpus(page, name) {
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(() => document.getElementById("file") && !document.getElementById("file").disabled, null, { timeout: 45_000 });
  await page.locator("#file").setInputFiles(corpus(name));
  await expect.poll(() => page.evaluate(async () => (await window.opendoc.ai.read({ tool: "search", query: "Intro" })).value.hits.length), { timeout: 45_000 }).toBeGreaterThan(0);
}

async function askInPanel(page, question) {
  await waitForAssistant(page);
  await page.evaluate(() => window.opendoc.execute("plugin.org.casualoffice.assistant.ask"));
  await expect(page.locator("#pluginPanel")).toBeVisible();
  await rules(page, []);
  const frame = assistant(page);
  await frame.locator("#prompt").fill(question);
  await frame.locator("#prompt").press("Enter");
  await expect.poll(async () => (await seen(page)).length, { timeout: 30_000 }).toBeGreaterThan(0);
  return (await seen(page))[0].messages.at(-1).content;
}

test("a question reaches the model with the table's cells and the footnote, each marked", async ({ page }) => {
  await mockProvider(page);
  await openCorpus(page, "real-producer-footnotes.docx");
  const prompt = await askInPanel(page, "What is in the table?");
  expect(prompt).toMatch(/<document_text id="t\d+" in="table">[\s\S]*R2C2[\s\S]*<\/document_text>/);
  expect(prompt).toMatch(/\| \[p\d+\] R1C1 \| \[p\d+\] R1C2 \|/);
  expect(prompt).toMatch(/<document_text id="p\d+" in="footnote">A footnote body\.<\/document_text>/);
});

test("a question reaches the model with the header and the footer, each marked", async ({ page }) => {
  await mockProvider(page);
  await openCorpus(page, "real-producer-header-footer.docx");
  const prompt = await askInPanel(page, "What does the header say?");
  expect(prompt).toMatch(/<document_text id="p\d+" in="header">Page header<\/document_text>/);
  expect(prompt).toMatch(/<document_text id="p\d+" in="footer">Page footer<\/document_text>/);
  expect(prompt).toMatch(/R2C2/);
});
