// Casual AI's docked panel (`docs/183` §21), against a MOCK provider (a
// Playwright route speaking OpenAI Chat Completions with SSE). No real model.
//
// The owner's review (2026-10-31): "too crowded, redundant information, too
// narrow, we need something tangible that basically works; it would be nice to
// have it on the right side, full page height, and drag to expand the width."
// Each of those is a guarantee here:
//
//   * docked on the RIGHT, from below the ribbon to the bottom of the window, and
//     the page column shrinks so nothing covers a page;
//   * a splitter on its inner edge — pointer drag, arrow keys, Home/End, double-
//     click to reset — clamped between 320px and half the window, remembered;
//   * one header line, the conversation, ONE composer; four suggestions only
//     while the conversation is empty;
//   * every answer is a card whose primary action CHANGES THE DOCUMENT (live, by
//     the co-editor, as a tracked suggestion by "Casual AI", `docs/183` §23).
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, stableBox } from "./fixtures.mjs";

test.describe.configure({ timeout: 120_000 });

const ASSISTANT = 'iframe[data-plugin="org.casualoffice.assistant"]';
const WORD = "CASUALPROBE";

async function withProvider(page) {
  await page.addInitScript(() => {
    try {
      localStorage.setItem(
        "opendoc.ai.provider",
        JSON.stringify({ preset: "custom", protocol: "openai", baseUrl: `${location.origin}/mock-ai/v1`, model: "mock-model", apiKey: "sk-test-not-real" }),
      );
    } catch {
      /* the plugin frame has no storage */
    }
  });
}

/** The mock model: `reply(body)` → the answer text. */
async function mockModel(page, reply) {
  const seen = [];
  await page.route("**/mock-ai/v1/chat/completions", async (route) => {
    const body = route.request().postDataJSON();
    seen.push(body);
    const text = reply(body);
    if (!body.stream) {
      await route.fulfill({ contentType: "application/json", body: JSON.stringify({ id: "x", object: "chat.completion", created: 1, model: "mock-model", choices: [{ index: 0, message: { role: "assistant", content: text }, finish_reason: "stop" }] }) });
      return;
    }
    const pieces = text.match(/.{1,6}/gs) ?? [text];
    const chunks = pieces.map((piece) => `data: ${JSON.stringify({ id: "x", model: "mock-model", choices: [{ index: 0, delta: { content: piece }, finish_reason: null }] })}\n\n`);
    chunks.push(`data: ${JSON.stringify({ id: "x", model: "mock-model", choices: [{ index: 0, delta: {}, finish_reason: "stop" }] })}\n\n`, "data: [DONE]\n\n");
    await route.fulfill({ contentType: "text/event-stream", body: chunks.join("") });
  });
  return seen;
}

const assistant = (page) => page.frameLocator(ASSISTANT);

async function openPanel(page) {
  await expect(page.locator(ASSISTANT)).toHaveCount(1, { timeout: 30_000 });
  await expect(assistant(page).locator("html")).toHaveAttribute("data-ready", "1", { timeout: 30_000 });
  await page.evaluate(() => window.opendoc.execute("plugin.org.casualoffice.assistant.ask"));
  await expect(page.locator("#pluginPanel")).toBeVisible();
}

const panelWidth = (page) => page.evaluate(() => Math.round(document.getElementById("pluginPanel").getBoundingClientRect().width));

const has = (page, word) => page.evaluate(async (w) => (await window.opendoc.ai.read({ tool: "search", query: w })).value.hits.length, word);

/** The document's own text, read the way the assistant reads it. */
const documentText = (page) =>
  page.evaluate(async () => (await window.opendoc.ai.read({ tool: "read", maxTokens: 8000 })).value.blocks.map((b) => b.text ?? "").join("\n"));

/** Types WORD on a new paragraph after the first, and selects it. */
async function selectWord(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type(WORD);
  for (let i = 0; i < WORD.length; i += 1) await page.keyboard.press("Shift+ArrowLeft");
}

/** A caret at the start of the document, in its first paragraph. */
async function caretAtStart(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
}

/** The co-editor finished writing the turn: Accept all in its bar (`docs/183`
 *  §23 replaced the private preview for Casual AI's own edits). */
async function turnDone(page) {
  await expect(page.locator("#aiTurnBar")).toHaveAttribute("data-state", "done", { timeout: 30_000 });
}
async function acceptTurn(page) {
  await expect(page.locator("#aiTurnBar")).toHaveAttribute("data-state", "done", { timeout: 30_000 });
  await page.locator("#aiTurnBar .ai-turn-accept").click();
  await expect(page.locator("#aiTurnBar")).toBeHidden();
}

// ---- Placement ------------------------------------------------------------------

for (const size of [{ width: 1280, height: 800 }, { width: 1920, height: 1080 }]) {
  test(`docked on the right at full height, and no page under it (${size.width}x${size.height})`, async ({ page }) => {
    await page.setViewportSize(size);
    await withProvider(page);
    await mockModel(page, () => "OK");
    await gotoEditor(page);
    await openPanel(page);
    const m = await page.evaluate(() => {
      const box = (el) => el.getBoundingClientRect();
      const panel = box(document.getElementById("pluginPanel"));
      const ribbon = document.querySelector(".workarea").getBoundingClientRect().top;
      const viewport = box(document.getElementById("viewport"));
      const footer = box(document.querySelector(".footer"));
      const pages = [...document.querySelectorAll(".page-wrap")].map((el) => box(el)).filter((r) => r.width > 0);
      return {
        panel: { left: panel.left, right: panel.right, top: panel.top, bottom: panel.bottom, width: panel.width },
        windowWidth: window.innerWidth,
        windowHeight: window.innerHeight,
        workareaTop: ribbon,
        viewportRight: viewport.right,
        footerRight: footer.right,
        pagesRight: Math.max(...pages.map((r) => r.right)),
        pageCount: pages.length,
      };
    });
    // The right edge of the window, from below the ribbon to the bottom.
    expect(Math.abs(m.panel.right - m.windowWidth), "flush to the window's right edge").toBeLessThanOrEqual(1);
    expect(Math.abs(m.panel.top - m.workareaTop), "starts right below the ribbon").toBeLessThanOrEqual(1);
    expect(Math.abs(m.panel.bottom - m.windowHeight), "reaches the bottom of the window").toBeLessThanOrEqual(1);
    expect(m.panel.left, "on the right half of the window").toBeGreaterThan(m.windowWidth / 2);
    // Nothing overlaps: the page column and the status bar end where it begins.
    expect(m.pageCount).toBeGreaterThan(0);
    expect(m.viewportRight).toBeLessThanOrEqual(m.panel.left + 1);
    expect(m.pagesRight).toBeLessThanOrEqual(m.panel.left);
    expect(m.footerRight).toBeLessThanOrEqual(m.panel.left + 1);
    expect(Math.round(m.panel.width)).toBe(400);
    // One header line: the title, ⋯ and Close.
    await expect(page.locator("#pluginPanel .panel-head button:visible")).toHaveCount(2);
    await expect(page.locator("#pluginPanelMore")).toBeVisible();
  });
}

test("the ⋯ menu holds Clear history and where text goes; the empty state's suggestions go once you start", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await withProvider(page);
  await mockModel(page, () => "An answer.");
  await gotoEditor(page);
  await openPanel(page);
  const frame = assistant(page);
  await expect(frame.locator("[data-chip] > span:last-child")).toHaveText(["Summarise", "Key points", "Draft…", "Review document", "Table…"]);
  // The endpoint is said once, on first use, not as a permanent line.
  await expect(frame.locator("#firstRun")).toBeVisible();
  await expect(frame.locator("#firstRun")).toContainText("127.0.0.1");
  await expect(frame.locator("#caveat")).toHaveText(/AI can make mistakes/);
  await frame.locator("#prompt").fill("Anything?");
  await frame.locator("#prompt").press("Enter");
  await expect(frame.locator(".ca-card")).toHaveCount(1);
  await expect(frame.locator("[data-chip]").first()).toBeHidden();
  await expect(frame.locator("#firstRun")).toBeHidden();
  await page.locator("#pluginPanelMore").click();
  const menu = page.locator("#pluginPanelMenu");
  await expect(menu).toBeVisible();
  await expect(menu.locator('[data-panel-menu="disclosure"]')).toContainText("127.0.0.1");
  await expect(menu.locator('[data-panel-menu="aiSettings"]')).toBeVisible();
  await menu.locator(".menu-item", { hasText: "Clear history" }).click();
  await expect(frame.locator(".ca-card")).toHaveCount(0);
  await expect(frame.locator("[data-chip]").first()).toBeVisible();
  // The note does not come back: it was shown once on this device.
  await expect(frame.locator("#firstRun")).toBeHidden();
});

// ---- The splitter -----------------------------------------------------------------

test("a drag on the inner edge changes the width, clamped to 320px and half the window, and it is remembered", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await withProvider(page);
  await mockModel(page, () => "OK");
  await gotoEditor(page);
  await openPanel(page);
  const handle = page.locator("#pluginPanelResize");
  await expect(handle).toHaveAttribute("role", "separator");
  await expect(handle).toHaveAttribute("aria-valuemin", "320");
  await expect(handle).toHaveAttribute("aria-valuemax", "640");
  await expect(handle).toHaveAttribute("aria-valuenow", "400");
  const drag = async (dx) => {
    const box = await stableBox(handle);
    const x = box.x + box.width / 2;
    const y = box.y + box.height / 2;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + dx / 2, y, { steps: 4 });
    await page.mouse.move(x + dx, y, { steps: 4 });
    await page.mouse.up();
  };
  await drag(-150);
  expect(await panelWidth(page)).toBe(550);
  await expect(handle).toHaveAttribute("aria-valuenow", "550");
  // The page column gave up the width: still nothing under the panel.
  // (A page wider than what is left scrolls inside the page column, as in Word;
  // it is clipped by that column, never drawn under the panel.)
  const clear = await page.evaluate(() => {
    const panel = document.getElementById("pluginPanel").getBoundingClientRect();
    const viewport = document.getElementById("viewport").getBoundingClientRect();
    const y = panel.top + panel.height / 2;
    const hit = document.elementFromPoint(panel.left + 8, y);
    return { columnEnds: viewport.right <= panel.left + 1, panelOnTop: document.getElementById("pluginPanel").contains(hit) };
  });
  expect(clear).toEqual({ columnEnds: true, panelOnTop: true });
  await drag(-900);
  expect(await panelWidth(page), "never past half the window").toBe(640);
  await drag(900);
  expect(await panelWidth(page), "never below the minimum").toBe(320);
  await drag(-180);
  expect(await panelWidth(page)).toBe(500);
  // Remembered on this device.
  await page.reload();
  await openPanel(page);
  expect(await panelWidth(page)).toBe(500);
  // A double-click puts it back.
  await page.locator("#pluginPanelResize").dblclick();
  expect(await panelWidth(page)).toBe(400);
});

test("the splitter answers the keyboard: arrows, Shift, Home and End, and says its value", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await withProvider(page);
  await mockModel(page, () => "OK");
  await gotoEditor(page);
  await openPanel(page);
  const handle = page.locator("#pluginPanelResize");
  await handle.focus();
  await page.keyboard.press("ArrowLeft");
  await expect(handle).toHaveAttribute("aria-valuenow", "416");
  expect(await panelWidth(page)).toBe(416);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  expect(await panelWidth(page)).toBe(384);
  await page.keyboard.press("Shift+ArrowLeft");
  expect(await panelWidth(page)).toBe(448);
  await page.keyboard.press("End");
  expect(await panelWidth(page)).toBe(640);
  await page.keyboard.press("ArrowLeft");
  expect(await panelWidth(page), "End is the maximum").toBe(640);
  await page.keyboard.press("Home");
  expect(await panelWidth(page)).toBe(320);
  await page.keyboard.press("ArrowRight");
  expect(await panelWidth(page), "Home is the minimum").toBe(320);
  await expect(handle).toHaveAttribute("aria-valuenow", "320");
  // A smaller window lowers the ceiling at once.
  await page.keyboard.press("End");
  await page.setViewportSize({ width: 1100, height: 800 });
  await expect.poll(() => panelWidth(page)).toBe(550);
});

// ---- Every answer action changes the document ------------------------------------

test("Insert at the cursor puts the answer into the document", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await withProvider(page);
  await mockModel(page, () => "CASUALINSERTED [p1]");
  await gotoEditor(page);
  await caretAtStart(page);
  await openPanel(page);
  const frame = assistant(page);
  await frame.locator("#prompt").fill("Give me one word");
  await frame.locator("#send").click();
  const card = frame.locator(".ca-card").last();
  await expect(card.locator(".ca-card-actions .btn-primary")).toHaveText("Insert at the cursor");
  expect(await has(page, "CASUALINSERTED")).toBe(0);
  await card.locator('[data-action="insert"]').click();
  await acceptTurn(page);
  await expect.poll(() => has(page, "CASUALINSERTED")).toBeGreaterThan(0);
  // Into the text at the caret, without the citation marker.
  expect(await documentText(page)).toMatch(/^CASUALINSERTED/);
  expect(await documentText(page)).not.toContain("[p1]");
});

test("with a selection attached, the answer card shows a diff and Replace selection replaces it", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await withProvider(page);
  const seen = await mockModel(page, () => "CASUALREPLACED");
  await gotoEditor(page);
  await openPanel(page);
  await selectWord(page);
  const frame = assistant(page);
  // The chip names the selection only while there is one.
  await expect(frame.locator("#attached")).toBeVisible();
  await expect(frame.locator("#attachedText")).toContainText(WORD);
  await frame.locator("#prompt").fill("Rename it");
  await frame.locator("#send").click();
  const card = frame.locator(".ca-card").last();
  await expect(card.locator(".ca-diff del")).toContainText(WORD);
  await expect(card.locator(".ca-diff ins")).toContainText("CASUALREPLACED");
  await expect(card.locator(".ca-card-actions .btn-primary")).toHaveText("Replace selection");
  // The selection went to the model as data.
  expect(seen.at(-1).messages.at(-1).content).toContain(`<document_text id="selection">${WORD}</document_text>`);
  // Detaching the selection is one click (the card keeps the one it answered).
  await frame.locator("#detach").click();
  await expect(frame.locator("#attached")).toBeHidden();
  await card.locator('[data-action="replace"]').click();
  await turnDone(page);
  await expect.poll(() => has(page, "CASUALREPLACED")).toBeGreaterThan(0);
  const changes = await page.evaluate(async () => (await window.opendoc.ai.read({ tool: "trackedChanges" })).value.trackedChanges);
  expect(changes.length).toBeGreaterThan(0);
});

test("Add as comment puts the answer on the document as a comment", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await withProvider(page);
  await mockModel(page, () => "CASUALCOMMENT worth checking.");
  await gotoEditor(page);
  await caretAtStart(page);
  await openPanel(page);
  const frame = assistant(page);
  await frame.locator("#prompt").fill("What should I check?");
  await frame.locator("#send").click();
  const card = frame.locator(".ca-card").last();
  await expect(card.locator('[data-action="comment"]')).toHaveText("Add as comment");
  const comments = () => page.evaluate(async () => (await window.opendoc.ai.read({ tool: "comments" })).value.comments.map((c) => c.text));
  expect((await comments()).some((t) => t.includes("CASUALCOMMENT"))).toBe(false);
  await card.locator('[data-action="comment"]').click();
  await acceptTurn(page);
  await expect.poll(async () => (await comments()).some((t) => t.includes("CASUALCOMMENT"))).toBe(true);
});

test("a review shows each suggestion as a diff, and Apply as suggestions lands them tracked", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await withProvider(page);
  await gotoEditor(page);
  const first = await page.evaluate(async () => {
    const read = await window.opendoc.ai.read({ tool: "read", maxTokens: 4000 });
    return read.value.blocks.find((b) => b.kind === "paragraph" && /\w{4,}/.test(b.text)).text.match(/\w{4,}/)[0];
  });
  await mockModel(page, () => JSON.stringify([{ id: "p1", quote: first, comment: "Clarify this.", replacement: `${first}CASUALREVIEWED` }]));
  await openPanel(page);
  const frame = assistant(page);
  await frame.locator('[data-chip="review"]').click();
  const card = frame.locator(".ca-card").last();
  await expect(card.locator(".ca-diff ins")).toContainText("CASUALREVIEWED");
  await expect(card.locator(".ca-review-comment")).toContainText("Clarify this.");
  await expect(card.locator(".ca-card-note")).toContainText("Comments: 1. Suggestions: 1.");
  await expect(card.locator(".ca-card-actions .btn-primary")).toHaveText("Apply as suggestions");
  // Nothing has changed yet: the card is shown before anything is applied.
  await expect(page.locator("#aiPreviewPanel")).toBeHidden();
  expect(await has(page, "CASUALREVIEWED")).toBe(0);
  await card.locator('[data-action="apply"]').click();
  await turnDone(page);
  await expect.poll(() => has(page, "CASUALREVIEWED")).toBeGreaterThan(0);
  const authors = await page.evaluate(async () => (await window.opendoc.ai.read({ tool: "trackedChanges" })).value.trackedChanges.map((c) => c.author));
  expect(authors.length).toBeGreaterThan(0);
  expect(authors.every((a) => /AI/.test(a ?? ""))).toBe(true);
});

test("'Write me a paragraph based on our discussion' draws on the conversation, and it streams into the document", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await withProvider(page);
  const seen = await mockModel(page, (body) => (body.messages[0].content.includes("drafting text") ? "CASUALDISCUSSED paragraph." : "The SDK ships in June."));
  await gotoEditor(page);
  await caretAtStart(page);
  await openPanel(page);
  const frame = assistant(page);
  await frame.locator("#prompt").fill("What ships in June?");
  await frame.locator("#send").click();
  await expect(frame.locator(".ca-card")).toHaveCount(1);
  await expect(frame.locator(".ca-card").last()).toContainText("The SDK ships in June.");
  // Draft… puts the composer in draft mode, said by a chip that undoes it.
  await frame.locator('.ca-card').last().locator('[data-action="insert"]').waitFor();
  await page.evaluate(() => window.opendoc.execute("plugin.org.casualoffice.assistant.draft"));
  await expect(frame.locator("#draftMode")).toBeVisible();
  await frame.locator("#prompt").fill("Write me a paragraph based on our discussion");
  await frame.locator("#prompt").press("Enter");
  await expect(frame.locator("#draftMode")).toBeHidden();
  // A draft streams straight into the document now (`docs/183` §23).
  await acceptTurn(page);
  const draft = seen.at(-1);
  expect(draft.messages.map((m) => m.content)).toEqual(expect.arrayContaining(["What ships in June?", "The SDK ships in June."]));
  await expect.poll(() => has(page, "CASUALDISCUSSED")).toBeGreaterThan(0);
});

test("Copy puts the answer on the clipboard, without citation markers", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.setViewportSize({ width: 1280, height: 800 });
  await withProvider(page);
  await mockModel(page, () => "CASUALCOPIED text [p1].");
  await gotoEditor(page);
  await openPanel(page);
  const frame = assistant(page);
  await frame.locator("#prompt").fill("Something to copy");
  await frame.locator("#send").click();
  await frame.locator('.ca-card [data-action="copy"]').last().click();
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("CASUALCOPIED text.");
});
