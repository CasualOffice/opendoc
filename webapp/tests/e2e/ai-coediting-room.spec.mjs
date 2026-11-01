// Casual AI in a SHARED room (`docs/178` §10, `docs/183` §24, RM-52): two real
// browsers joined through `examples/share-host.mjs` and its relay, in a GRANTED
// room so both names are the ones the host signed.
//
// What the other person must see while Ada's assistant writes, and after:
//   * the AI's own caret and flag, "Casual AI · Ada Lovelace", moving as it writes
//     and gone when the turn ends — not just text appearing from nowhere;
//   * the suggestion attributed to the AGENT PRINCIPAL, "Casual AI on behalf of Ada
//     Lovelace", not to Ada (before, every replica's conform restamped it to her);
//   * both through the ordinary OT path: Bob's text equals Ada's.
//
// Needs `target/debug/opendoc-relay` (`cargo build -p opendoc --bin opendoc-relay`).
import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test, expect } from "./fixtures.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const TEXT = ["Alpha line the assistant writes after.", "Bravo line in the middle.", "Charlie line at the end."].join("\n");
const ASSISTANT = 'iframe[data-plugin="org.casualoffice.assistant"]';
const ANSWER = "ALPHA the assistant writes this sentence into the shared document a few words at a time, OMEGA.";
const AGENT = "Casual AI on behalf of Ada Lovelace";

test.describe.configure({ timeout: 180_000 });

let host, origin;
test.beforeAll(async () => {
  host = spawn(process.execPath, [resolve(root, "examples/share-host.mjs")], {
    cwd: root, env: { ...process.env, OPENDOC_SHARE_PORT: "0", OPENDOC_SHARE_ACCESS: "granted" }, stdio: ["ignore", "pipe", "pipe"] });
  origin = await new Promise((done, fail) => {
    const timeout = setTimeout(() => fail(new Error("Share host startup timed out")), 10000);
    host.once("error", fail);
    host.once("exit", (code) => { clearTimeout(timeout); fail(new Error(`Share host exited: ${code}`)); });
    host.stdout.on("data", (bytes) => { const match = /Share host listening on (\d+)/.exec(String(bytes)); if (match) { clearTimeout(timeout); done(`http://127.0.0.1:${match[1]}`); } });
  });
});
test.afterAll(async () => {
  if (host && host.exitCode === null) { const exited = new Promise((done) => host.once("exit", done)); host.kill(); await exited; }
});

async function ready(page) {
  await expect(page.locator(".page-wrap").first()).toBeVisible({ timeout: 45000 });
  await page.waitForFunction(() => document.body.dataset.fontsReady === "true");
}
async function identify(page, identity) {
  await page.waitForFunction(() => typeof window.opendoc?.identify === "function");
  const answer = await page.evaluate((value) => window.opendoc.identify(value), identity);
  expect(answer.ok, JSON.stringify(answer)).toBe(true);
}
async function person(browser, cookie) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  await context.addCookies([{ name: "opendoc_dev_user", value: encodeURIComponent(cookie), url: origin }]);
  return context;
}

/** A provider pointed at a mock endpoint that really STREAMS (Ada's tab only). */
async function streamingProvider(page) {
  await page.addInitScript(() => {
    try {
      localStorage.setItem("opendoc.ai.provider", JSON.stringify({ preset: "custom", protocol: "openai", baseUrl: `${location.origin}/mock-ai/v1`, model: "mock-model", apiKey: "sk-test-not-real" }));
    } catch {
      /* the plugin frame has no storage */
    }
    const real = window.fetch.bind(window);
    const mock = { rules: [], fallback: "OK", delay: 120, size: 5 };
    window.__mockAi = mock;
    window.fetch = async (input, init = {}) => {
      const url = typeof input === "string" ? input : input.url;
      if (!String(url).includes("/mock-ai/v1/chat/completions")) return real(input, init);
      const body = JSON.parse(init.body);
      const all = body.messages.map((m) => m.content).join("\n");
      const rule = mock.rules.find((r) => new RegExp(r.match).test(all));
      const text = rule ? rule.text : mock.fallback;
      if (!body.stream) {
        return new Response(JSON.stringify({ id: "x", object: "chat.completion", created: 1, model: "mock-model", choices: [{ index: 0, message: { role: "assistant", content: text }, finish_reason: "stop" }] }), { headers: { "content-type": "application/json" } });
      }
      const pieces = text.match(new RegExp(`[\\s\\S]{1,${mock.size}}`, "g")) ?? [text];
      const encoder = new TextEncoder();
      const stream = new ReadableStream({
        async start(controller) {
          for (const piece of pieces) {
            await new Promise((done) => setTimeout(done, mock.delay));
            if (init.signal?.aborted) { controller.error(new DOMException("Aborted", "AbortError")); return; }
            controller.enqueue(encoder.encode(`data: ${JSON.stringify({ id: "x", model: "mock-model", choices: [{ index: 0, delta: { content: piece }, finish_reason: null }] })}\n\n`));
          }
          controller.enqueue(encoder.encode(`data: ${JSON.stringify({ id: "x", model: "mock-model", choices: [{ index: 0, delta: {}, finish_reason: "stop" }] })}\n\ndata: [DONE]\n\n`));
          controller.close();
        },
      });
      return new Response(stream, { headers: { "content-type": "text/event-stream" } });
    };
  });
}

const visibleText = (page) => page.locator("#a11yDocument").innerText();

test("Casual AI writes into a shared room as its own participant: its caret moves on the other screen, and its suggestion is the agent's, not the person's", async ({ browser }) => {
  const ada = await person(browser, "u_ada|Ada Lovelace");
  const bob = await person(browser, "u_bob|Bob Stone");
  try {
    // Ada opens the document with a provider configured, and shares it.
    const a = await ada.newPage();
    await streamingProvider(a);
    await a.goto(`${origin}/editor.html`); await ready(a);
    await identify(a, { id: "u_ada", name: "Ada Lovelace" });
    await a.locator("#file").setInputFiles({ name: "room.txt", mimeType: "text/plain", buffer: Buffer.from(TEXT) });
    if (await a.locator("#confirmDialog").isVisible().catch(() => false)) await a.locator("#confirmAccept").click();
    await expect(a.locator("#a11yDocument")).toContainText("Bravo line");
    await a.locator("#shareBtn").click();
    const dialog = a.locator(".shared-link-dialog"); await expect(dialog).toBeVisible();
    const link = await dialog.locator("input").inputValue();
    await dialog.getByRole("button", { name: "Close", exact: true }).click();

    // Bob joins, with no provider: nothing of the AI exists in his editor.
    const b = await bob.newPage();
    const url = new URL(link); url.searchParams.set("awaitIdentity", "1");
    await b.goto(url.href);
    await identify(b, { id: "u_bob", name: "Bob Stone" });
    await ready(b);
    await expect(b.locator("#collabSync")).toHaveAttribute("data-state", "acknowledged");
    await expect(b.locator(ASSISTANT)).toHaveCount(0);

    // Ada asks her assistant to draft at the end of the first line.
    await expect(a.locator(ASSISTANT)).toHaveCount(1, { timeout: 30_000 });
    const frame = a.frameLocator(ASSISTANT);
    await expect(frame.locator("html")).toHaveAttribute("data-ready", "1", { timeout: 30_000 });
    await a.evaluate(() => { window.__mockAi.rules = [{ match: "drafting text", text: "" }]; });
    await a.evaluate((text) => { window.__mockAi.rules = [{ match: "drafting text", text }]; }, ANSWER);
    await a.evaluate(() => window.opendoc.execute("plugin.org.casualoffice.assistant.ask"));
    await expect(a.locator("#pluginPanel")).toBeVisible();
    await a.locator(".page-wrap .page").first().click({ position: { x: 60, y: 60 } });
    await a.keyboard.press("ControlOrMeta+Home");
    await a.keyboard.press("End");
    await frame.locator('[data-chip="draft"]').click();
    await frame.locator("#prompt").fill("Write a sentence");
    // Record every position the AI's caret takes on BOB's screen, frame by frame,
    // from before the turn starts. Polling from the test every 150 ms missed the
    // movement on CI, where the whole mock answer streamed between two samples.
    await b.evaluate(() => {
      const seen = new Set();
      window.__aiCaretPositions = seen;
      const tick = () => {
        const el = document.querySelector('.remote-caret[data-agent="casual-ai"]');
        if (el) {
          const r = el.getBoundingClientRect();
          seen.add(`${Math.round(r.x)}:${Math.round(r.y)}`);
        }
        if (window.__aiCaretPositions === seen) requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
    await frame.locator("#prompt").press("Enter");

    // On BOB's screen: the AI's caret and flag, named for whom it writes, moving.
    const aiCaret = b.locator('.remote-caret[data-agent="casual-ai"]');
    await expect(aiCaret).toHaveCount(1, { timeout: 30_000 });
    await expect(aiCaret.locator("span")).toHaveText("Casual AI · Ada Lovelace");
    await expect
      .poll(() => b.evaluate(() => window.__aiCaretPositions.size), { timeout: 30_000 })
      .toBeGreaterThanOrEqual(2);
    const positions = new Set(await b.evaluate(() => [...window.__aiCaretPositions]));
    expect(positions.size, `the AI's caret moved on the other screen: ${[...positions].join(" ")}`).toBeGreaterThanOrEqual(2);
    // Ada's own caret is still Ada's.
    await expect(b.locator('.remote-caret:not([data-agent]) span').first()).toHaveText("Ada Lovelace");

    // The turn ends: the AI's caret goes, the text is on both screens.
    await expect(a.locator("#aiTurnBar")).toHaveAttribute("data-state", "done", { timeout: 45_000 });
    await expect(aiCaret).toHaveCount(0, { timeout: 20_000 });
    await expect(b.locator("#a11yDocument")).toContainText("OMEGA", { timeout: 20_000 });
    expect(await visibleText(b)).toBe(await visibleText(a));

    // The suggestion is the agent principal's, on Ada's screen and on Bob's.
    for (const page of [a, b]) {
      const card = page.locator("#reviewSidebar .review-margin-card.review-margin-insertion").filter({ hasText: "ALPHA" });
      await expect(card).toBeVisible({ timeout: 20_000 });
      await expect(card.locator(".review-margin-card-head strong")).toHaveText(AGENT);
    }

    // Reject all on Ada's turn bar takes it back on both screens.
    await a.locator("#aiTurnBar .ai-turn-reject").click();
    await expect(b.locator("#a11yDocument")).not.toContainText("ALPHA", { timeout: 20_000 });
  } finally { await bob.close(); await ada.close(); }
});
